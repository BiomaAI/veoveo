//! The hub spooler: an embedded Rerun gRPC proxy whose every message is also
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
use veoveo_recording_hub::config::{DatasetName, DatasetRoute, SpoolerConfig};
use veoveo_recording_hub::spool::{Spooler, run_blocking};

#[derive(Parser, Debug)]
#[command(name = "spooler", about = "Recording Hub durable spooler + gRPC proxy")]
struct Args {
    /// gRPC ingest bind address (the embedded proxy).
    #[arg(long, default_value = "127.0.0.1:9876")]
    bind: SocketAddr,
    /// Root directory for `{dataset}/{day}/{recording}.rrd`.
    #[arg(long)]
    spool_dir: PathBuf,
    /// Routing rule `dataset=application_id_prefix` (repeatable). An empty
    /// prefix (`dataset=`) is the catch-all.
    #[arg(long = "route")]
    routes: Vec<String>,
    #[arg(long, default_value_t = 256 * 1024 * 1024)]
    segment_max_bytes: u64,
    #[arg(long, default_value_t = 3600)]
    segment_max_age_s: u64,
    #[arg(long, default_value_t = 250)]
    flush_interval_ms: u64,
    #[arg(long, default_value_t = 1024 * 1024 * 1024)]
    live_queue_limit_bytes: u64,
    /// Path to the `rerun` CLI used to verify frozen segments.
    #[arg(long)]
    rerun_bin: Option<PathBuf>,
    /// Write a readiness marker file once the proxy is accepting traffic.
    #[arg(long)]
    ready_file: Option<PathBuf>,
    /// Log aggregate counters every N seconds.
    #[arg(long, default_value_t = 10)]
    counters_interval_s: u64,
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

    let config = SpoolerConfig {
        bind: args.bind,
        spool_dir: args.spool_dir.clone(),
        datasets,
        segment_max_bytes: args.segment_max_bytes,
        segment_max_age_s: args.segment_max_age_s,
        flush_interval_ms: args.flush_interval_ms,
        live_queue_limit_bytes: args.live_queue_limit_bytes,
        rerun_bin: args.rerun_bin.clone(),
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

    // Trip the shutdown flag on Ctrl-C / SIGTERM.
    let stopping = Arc::new(AtomicBool::new(false));
    {
        let stopping = stopping.clone();
        tokio::spawn(async move {
            wait_for_shutdown().await;
            stopping.store(true, Ordering::SeqCst);
        });
    }

    // The receiver is a synchronous channel; drain it on a blocking thread so
    // the async runtime stays free for the tonic server.
    let stopping_drain = stopping.clone();
    let drain = tokio::task::spawn_blocking(move || -> Result<veoveo_recording_hub::Counters> {
        let spooler = Spooler::new(config)?;
        run_blocking(
            spooler,
            receiver,
            stopping_drain,
            flush_interval,
            counters_interval,
        )
    });

    // Wait for shutdown, then tear the proxy down and join the drain.
    while !stopping.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    signal.stop();
    let counters = drain.await.context("drain task panicked")??;
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
