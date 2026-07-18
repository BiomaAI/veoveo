//! The hub spooler: authenticated batch ingest plus a loopback Rerun receiver
//! whose every materialized message is also
//! persisted durably to day-partitioned segment files. Because the proxy and
//! the writer live in one process, the durable write is the first-class path —
//! there is no reconnect window in which the ring buffer could drop data a
//! subscribing spooler never saw.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use re_grpc_server::{MemoryLimit, ServerOptions, shutdown};
use secrecy::{ExposeSecret, SecretString};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalResourceTokenVerifier,
    GatewayInternalTrustBundle, ProtectedResourceId, ServerSlug, TokenIssuer,
};
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials};
use veoveo_recording_hub::config::{DatasetName, DatasetRoute, SpoolerConfig};
use veoveo_recording_hub::spool::{Spooler, run_blocking};
use veoveo_recording_hub::{
    CatalogPolicy, PlatformCatalog, RecordingIngestService, RecordingIngestServiceConfig,
    recording_ingest_internal_router,
};

#[derive(Parser)]
#[command(name = "spooler", about = "Recording Hub durable spooler + gRPC proxy")]
struct Args {
    /// gRPC ingest bind address (the embedded proxy).
    #[arg(long, default_value = "127.0.0.1:9876")]
    bind: SocketAddr,
    /// Cluster-internal authenticated protobuf ingest bind address.
    #[arg(long, default_value = "127.0.0.1:9878")]
    internal_ingest_bind: SocketAddr,
    /// Root directory for `{dataset}/{day}/{recording}.rrd`.
    #[arg(long)]
    spool_dir: PathBuf,
    /// Durable batch journal. It must share the spool volume but remain a
    /// distinct directory from materialized RRD segments.
    #[arg(long, default_value = "/recordings/.ingest-journal")]
    journal_dir: PathBuf,
    /// Routing rule `dataset=application_id_prefix` (repeatable). An empty
    /// prefix (`dataset=`) is the catch-all.
    #[arg(long = "route")]
    routes: Vec<String>,
    #[arg(long, default_value_t = 192 * 1024 * 1024)]
    segment_max_bytes: u64,
    #[arg(long, default_value_t = 3600)]
    segment_max_age_s: u64,
    /// Finish and expose a recording after this many seconds without data.
    #[arg(long, default_value_t = 15)]
    recording_idle_timeout_s: u64,
    #[arg(long, default_value_t = 250)]
    flush_interval_ms: u64,
    /// Fsync live segments on every scheduled flush.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    fsync_on_flush: bool,
    #[arg(long, default_value_t = 1024 * 1024 * 1024)]
    live_queue_limit_bytes: u64,
    /// Write a readiness marker file once the proxy is accepting traffic.
    #[arg(long)]
    ready_file: Option<PathBuf>,
    /// Log aggregate counters every N seconds.
    #[arg(long, default_value_t = 10)]
    counters_interval_s: u64,
    /// Required database-scoped SurrealDB runtime connection. Schema migration
    /// is deliberately unavailable in this process.
    #[arg(long, env = "VEOVEO_SURREAL_ENDPOINT")]
    surreal_endpoint: String,
    #[arg(long, env = "VEOVEO_SURREAL_NAMESPACE")]
    surreal_namespace: String,
    #[arg(long, env = "VEOVEO_SURREAL_DATABASE")]
    surreal_database: String,
    #[arg(long, env = "VEOVEO_SURREAL_USERNAME")]
    surreal_username: String,
    #[arg(
        long,
        env = "VEOVEO_SURREAL_PASSWORD",
        hide_env_values = true,
        value_parser = parse_secret
    )]
    surreal_password: SecretString,
    #[arg(long, env = "RECORDING_TENANT_KEY")]
    recording_tenant_key: String,
    #[arg(long, env = "RECORDING_OWNER_KEY", default_value = "recording-hub")]
    recording_owner_key: String,
    #[arg(
        long,
        env = "RECORDING_OWNER_ISSUER",
        default_value = "https://veoveo.local/services"
    )]
    recording_owner_issuer: String,
    #[arg(long, env = "RECORDING_OWNER_SUBJECT", default_value = "recording-hub")]
    recording_owner_subject: String,
    #[arg(long, env = "RECORDING_CLASSIFICATION")]
    recording_classification: String,
    #[arg(
        long = "recording-label",
        env = "RECORDING_LABELS",
        value_delimiter = ','
    )]
    recording_labels: Vec<String>,
    #[arg(long, env = "RECORDING_INGEST_PROTECTED_RESOURCE")]
    ingest_protected_resource: String,
    #[arg(
        long,
        env = "VEOVEO_INTERNAL_TRUST_JWKS",
        hide_env_values = true,
        value_parser = parse_secret
    )]
    internal_trust_jwks: SecretString,
    #[arg(
        long,
        env = "VEOVEO_INTERNAL_TOKEN_ISSUER",
        default_value = GATEWAY_INTERNAL_TOKEN_ISSUER
    )]
    internal_token_issuer: String,
}

fn parse_secret(value: &str) -> Result<SecretString, String> {
    (!value.is_empty())
        .then(|| SecretString::from(value))
        .ok_or_else(|| "secret must not be empty".to_owned())
}

fn parse_route(raw: &str) -> Result<DatasetRoute> {
    let (dataset, prefix) = raw
        .split_once('=')
        .with_context(|| format!("route `{raw}` must be dataset=prefix"))?;
    Ok(DatasetRoute {
        dataset: DatasetName::new(dataset)?,
        application_id_prefix: prefix.to_string(),
    })
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();
    let datasets = args
        .routes
        .iter()
        .map(|r| parse_route(r))
        .collect::<Result<Vec<_>>>()?;

    let spool_dir = if args.spool_dir.is_absolute() {
        args.spool_dir.clone()
    } else {
        std::env::current_dir()?.join(&args.spool_dir)
    };
    let config = SpoolerConfig {
        bind: args.bind,
        spool_dir,
        datasets,
        segment_max_bytes: args.segment_max_bytes,
        segment_max_age_s: args.segment_max_age_s,
        recording_idle_timeout_s: args.recording_idle_timeout_s,
        flush_interval_ms: args.flush_interval_ms,
        fsync_on_flush: args.fsync_on_flush,
        live_queue_limit_bytes: args.live_queue_limit_bytes,
    };
    config.validate()?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(run(config, args))
}

async fn run(config: SpoolerConfig, args: Args) -> Result<()> {
    let flush_interval = config.flush_interval();
    let counters_interval = Duration::from_secs(args.counters_interval_s.max(1));

    let store = PlatformStore::connect(
        StoreConfig::builder(
            &args.surreal_endpoint,
            &args.surreal_namespace,
            &args.surreal_database,
            StoreCredentials::database(&args.surreal_username, args.surreal_password.clone()),
        )
        .build()?,
    )
    .await
    .context("connecting recording catalog with database-scoped credentials")?;
    let ingest = RecordingIngestService::new(
        store.clone(),
        RecordingIngestServiceConfig {
            journal_root: args.journal_dir.clone(),
            spool_root: config.spool_dir.clone(),
            protected_resource: ProtectedResourceId::new(&args.ingest_protected_resource)?,
            maximum_batch_bytes: veoveo_recording_protocol::DEFAULT_MAXIMUM_BATCH_BYTES,
            segment_max_bytes: config.segment_max_bytes,
            segment_max_age_seconds: config.segment_max_age_s,
        },
    )?;
    let reconciled_ingest = ingest.reconcile().await?;
    tracing::info!(reconciled_ingest, "recording ingest journal reconciled");
    let verifier = GatewayInternalResourceTokenVerifier::new(
        TokenIssuer::new(&args.internal_token_issuer)?,
        ServerSlug::new("recording-hub")?,
        GatewayInternalTrustBundle::from_json(args.internal_trust_jwks.expose_secret())?,
    );
    let ingest_router = recording_ingest_internal_router(
        ingest,
        verifier,
        veoveo_recording_protocol::DEFAULT_MAXIMUM_BATCH_BYTES,
    );
    let ingest_listener = tokio::net::TcpListener::bind(args.internal_ingest_bind).await?;
    let mut ingest_http = tokio::spawn(async move {
        axum::serve(ingest_listener, ingest_router)
            .await
            .context("serving Recording Hub internal ingest API")
    });
    tracing::info!(bind = %args.internal_ingest_bind, "recording hub internal ingest API up");

    let catalog = PlatformCatalog::new(
        store,
        config.spool_dir.clone(),
        CatalogPolicy {
            tenant_key: args.recording_tenant_key.clone(),
            owner_key: args.recording_owner_key.clone(),
            owner_issuer: args.recording_owner_issuer.clone(),
            owner_subject: args.recording_owner_subject.clone(),
            classification: args.recording_classification.clone(),
            labels: args.recording_labels.clone(),
        },
        tokio::runtime::Handle::current(),
    )
    .await?;
    let reconciled = catalog.reconcile().await?;
    tracing::info!(reconciled, "recording catalog reconciled");

    let (signal, shutdown) = shutdown::shutdown();
    let options = ServerOptions {
        memory_limit: MemoryLimit::from_bytes(config.live_queue_limit_bytes),
        ..Default::default()
    };
    let (receiver, _handle) = re_grpc_server::spawn_with_recv(config.bind, options, shutdown);
    tracing::info!(bind = %config.bind, spool = %config.spool_dir.display(), "hub spooler proxy up");

    if let Some(ready) = &args.ready_file {
        std::fs::write(ready, b"ready")
            .with_context(|| format!("writing ready file {}", ready.display()))?;
    }

    let stopping = Arc::new(AtomicBool::new(false));

    // The receiver is a synchronous channel; drain it on a blocking thread so
    // the async runtime stays free for the tonic server.
    let stopping_drain = stopping.clone();
    let mut drain =
        tokio::task::spawn_blocking(move || -> Result<veoveo_recording_hub::Counters> {
            let spooler = Spooler::new(config)?.with_catalog(catalog);
            run_blocking(
                spooler,
                receiver,
                stopping_drain,
                flush_interval,
                counters_interval,
            )
        });

    // The durable drain is part of readiness. If it exits before an operator
    // shutdown, stop accepting traffic and fail the process immediately.
    let counters = tokio::select! {
        _ = wait_for_shutdown() => {
            stopping.store(true, Ordering::SeqCst);
            signal.stop();
            ingest_http.abort();
            if let Some(ready) = &args.ready_file {
                let _ = std::fs::remove_file(ready);
            }
            drain.await.context("drain task panicked")??
        }
        result = &mut drain => {
            signal.stop();
            ingest_http.abort();
            if let Some(ready) = &args.ready_file {
                let _ = std::fs::remove_file(ready);
            }
            let counters = result.context("drain task panicked")??;
            anyhow::bail!(
                "durable drain exited before shutdown after {} messages",
                counters.messages
            );
        }
        result = &mut ingest_http => {
            stopping.store(true, Ordering::SeqCst);
            signal.stop();
            if let Some(ready) = &args.ready_file {
                let _ = std::fs::remove_file(ready);
            }
            result.context("recording ingest HTTP task panicked")??;
            anyhow::bail!("recording ingest HTTP server exited before shutdown");
        }
    };
    tracing::info!(
        messages = counters.messages,
        segments_frozen = counters.segments_frozen,
        "hub spooler stopped"
    );
    Ok(())
}

async fn wait_for_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        let mut int = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        tokio::select! {
            _ = term.recv() => {}
            _ = int.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
