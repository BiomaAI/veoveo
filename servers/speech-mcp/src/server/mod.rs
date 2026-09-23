mod admin;
mod auth;
mod config;
mod dictation;
mod host;
mod mcp;
mod tasks;

use crate::{application::SpeechService, process::WorkerProcess};
use axum::{Router, extract::State, http::StatusCode, middleware, routing::get};
use clap::Parser;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    PublicDeployment, ServerSlug, TokenIssuer, docs::ServerDocs, public_allowed_hosts,
};
use veoveo_task_runtime::{TaskRuntime, TaskRuntimeConfig};

pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("speech"));

pub async fn run() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _telemetry = veoveo_mcp_contract::init_server_telemetry("veoveo-speech-mcp", "info")?;
    let args = config::Args::parse();
    anyhow::ensure!(
        args.concurrent_recordings > 0
            && args.concurrent_recordings < usize::from(args.inference_capacity),
        "recording capacity must reserve at least one inference slot for dictation"
    );
    let deployment = PublicDeployment::new(&args.public_base_url)?;
    let endpoint = deployment.server("speech")?;
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new("speech")?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let worker = Arc::new(WorkerProcess::start(&args.python, args.inference_capacity).await?);
    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            args.surreal_endpoint,
            args.surreal_namespace,
            args.surreal_database,
            args.surreal_auth_level,
            args.surreal_username,
            args.surreal_password,
        ),
        "speech",
        format!("speech-{}", uuid::Uuid::now_v7()),
    )
    .await?;
    let service = Arc::new(SpeechService::new(
        tasks,
        HttpArtifactPlane::new(args.artifact_service_url),
        worker,
        args.concurrent_recordings,
        usize::from(args.inference_capacity) - args.concurrent_recordings,
    ));
    let stop = CancellationToken::new();
    let recovery = {
        let service = service.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            // Lease recovery is runtime maintenance and never initiates new model work.
            let mut tick = tokio::time::interval(Duration::from_secs(30));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! { () = stop.cancelled() => break, _ = tick.tick() => {} }
                match service.tasks.recover().await {
                    Ok(report) => {
                        for snapshot in report.resumable {
                            if let Err(error) = service.resume(snapshot).await {
                                tracing::warn!(%error, "Speech recovery could not claim work");
                            }
                        }
                    }
                    Err(error) => tracing::warn!(%error, "Speech recovery store unavailable"),
                }
                service.tasks.reap_workers().await;
            }
        })
    };
    let mut hosts = public_allowed_hosts(&deployment, args.allow_loopback_hosts);
    for host in args.allowed_hosts {
        anyhow::ensure!(
            veoveo_mcp_contract::parse_allowed_host_authority(&host).is_some(),
            "invalid allowed host"
        );
        hosts.push(host);
    }
    let router = router(
        service,
        verifier,
        hosts,
        endpoint.mount_path(),
        stop.clone(),
    );
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, args.port)).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown({
            let stop = stop.clone();
            async move {
                let mut terminate =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .expect("install SIGTERM handler");
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
                stop.cancel();
            }
        })
        .await?;
    stop.cancel();
    recovery.abort();
    Ok(())
}

/// Wire the same authenticated hosted boundary for the executable and native qualification.
pub fn router(
    service: Arc<SpeechService>,
    verifier: GatewayInternalTokenVerifier,
    hosts: Vec<String>,
    mount_path: &str,
    stop: CancellationToken,
) -> Router {
    let hosts = Arc::new(hosts.into_iter().collect::<Vec<_>>());
    let transport = StreamableHttpService::new(
        {
            let service = service.clone();
            move || Ok(mcp::SpeechMcp::new(service.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(hosts.iter().cloned())
            .with_cancellation_token(stop.child_token()),
    );
    let authenticated = auth::InternalMcpAuthState { verifier };
    let mcp = Router::new()
        .route_service("/", transport.clone())
        .route_service("/{*path}", transport)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn_with_state(
            authenticated.clone(),
            auth::authenticate_internal_mcp,
        ));
    let admin = admin::router().layer(middleware::from_fn_with_state(
        authenticated.clone(),
        auth::authenticate_internal_mcp,
    ));
    let dictation = dictation::router().layer(middleware::from_fn_with_state(
        authenticated,
        auth::authenticate_internal_mcp,
    ));
    let routes = Router::new()
        .merge(dictation)
        .route("/healthz", get(health))
        .route("/readyz", get(ready))
        .with_state(service)
        .nest("/mcp", mcp)
        .nest("/admin", admin);
    Router::new()
        .nest(mount_path, routes)
        .layer(middleware::from_fn_with_state(hosts, host::validate_host))
}

async fn ready(State(state): State<Arc<SpeechService>>) -> StatusCode {
    if state.worker.ready().await.is_ok()
        && matches!(
            tokio::time::timeout(
                Duration::from_secs(2),
                state.tasks.platform_store().healthcheck()
            )
            .await,
            Ok(Ok(()))
        )
    {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn health(State(state): State<Arc<SpeechService>>) -> StatusCode {
    if state.worker.ready().await.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
