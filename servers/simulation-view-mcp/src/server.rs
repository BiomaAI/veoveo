mod admin;
mod auth;
mod config;
mod host;
mod signaling;

use std::{net::SocketAddr, sync::Arc};

use anyhow::Result;
use axum::{Json, Router, http::StatusCode, middleware, routing::get};
use clap::Parser;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use serde::Serialize;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    ServerSlug, TelemetryGuard, TokenIssuer, init_server_telemetry, public_allowed_hosts,
};

use crate::{
    artifacts::SceneArtifactMaterializer,
    durability::SimulationViewRepository,
    mcp::{SimulationViewMcp, SimulationViewMcpState},
    reconciler::{ReconcilerConfig, spawn_reconciler},
    runtime::RuntimeClients,
    state::SimulationViewService,
};

pub(crate) use auth::ForwardedBearer;
use auth::{InternalAuthState, authenticate_internal};
use config::Args;
use host::validate_host;
use signaling::SignalingState;

pub(crate) const SERVER_SLUG: &str = "simulation-view";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Readiness {
    ready: bool,
    artifact_plane_ready: bool,
    store_ready: bool,
    durable_state_ready: bool,
    runtime: crate::runtime::SimulationViewReadiness,
}

pub async fn run() -> Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard = init_server_telemetry(
        "veoveo-simulation-view-mcp",
        "info,veoveo_simulation_view_mcp=debug",
    )?;
    let args = Args::parse();
    args.validate()?;
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let service = SimulationViewService::new(args.service_config()?)?;
    let store = veoveo_platform_store::PlatformStore::connect(args.store_config()?).await?;
    let repository = SimulationViewRepository::new(store);
    let restored_sessions = repository.restore(&service).await?;
    tracing::info!(
        restored_sessions,
        "restored durable Simulation View desired state"
    );
    let runtimes = Arc::new(RuntimeClients::new(
        &args.renderer_endpoint,
        &args.pose_endpoint,
        &args.renderer_control_token,
        &args.pose_control_token,
        &args.renderer_signaling_url,
        args.public_media_port,
    )?);
    let artifacts = SceneArtifactMaterializer::new(
        &args.artifact_service_url,
        &args.renderer_endpoint,
        &args.renderer_control_token,
    )?;
    let mcp_state = SimulationViewMcpState::new(
        service.clone(),
        runtimes.clone(),
        artifacts.clone(),
        repository.clone(),
        &args.public_signaling_url,
    )?;
    let signaling = SignalingState::new(
        service.clone(),
        mcp_state.clone(),
        &args.renderer_signaling_url,
        args.public_media_port,
    )?;

    let cancellation = tokio_util::sync::CancellationToken::new();
    spawn_reconciler(
        service.clone(),
        runtimes.clone(),
        repository.clone(),
        mcp_state.subscriptions.clone(),
        ReconcilerConfig {
            interval: std::time::Duration::from_secs(args.reconcile_interval_seconds),
            authorization_renewal_lead: std::time::Duration::from_secs(
                args.authorization_renewal_lead_seconds,
            ),
            retry_max: std::time::Duration::from_secs(args.reconcile_retry_max_seconds),
        },
        cancellation.child_token(),
    );
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts);
    let auth_state = InternalAuthState { verifier };
    let mcp_service = StreamableHttpService::new(
        {
            let state = mcp_state.clone();
            move || Ok(SimulationViewMcp::new(state.clone()))
        },
        veoveo_mcp_contract::canonical_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(cancellation.child_token()),
    );
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_internal,
        ));
    let admin_router = admin::router().layer(middleware::from_fn_with_state(
        auth_state,
        authenticate_internal,
    ));

    let readiness = runtimes.clone();
    let readiness_artifacts = artifacts;
    let readiness_repository = repository;
    let readiness_service = service;
    let service_router = Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route(
            "/readyz",
            get(move || {
                let readiness = readiness.clone();
                let artifacts = readiness_artifacts.clone();
                let repository = readiness_repository.clone();
                let service = readiness_service.clone();
                async move {
                    let (runtime, artifact_plane_ready, store_ready) =
                        tokio::join!(readiness.readiness(), artifacts.ready(), repository.ready());
                    let durable_state_ready = service.reconciliation_ready();
                    let report = Readiness {
                        ready: runtime.ready
                            && artifact_plane_ready
                            && store_ready
                            && durable_state_ready,
                        artifact_plane_ready,
                        store_ready,
                        durable_state_ready,
                        runtime,
                    };
                    let status = if report.ready {
                        StatusCode::OK
                    } else {
                        StatusCode::SERVICE_UNAVAILABLE
                    };
                    (status, Json(report))
                }
            }),
        )
        .route("/signaling", get(signaling::upgrade))
        .route("/signaling/{*tail}", get(signaling::upgrade))
        .with_state(signaling)
        .nest("/admin", admin_router)
        .nest("/mcp", mcp_router);
    let router = Router::new()
        .nest(public_endpoint.mount_path(), service_router)
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(
        service = "veoveo-simulation-view-mcp",
        %address,
        mcp_path = public_endpoint.path("mcp"),
        readiness_path = public_endpoint.path("readyz"),
        signaling_path = public_endpoint.path("signaling"),
        "listening"
    );
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation.cancel();
        })
        .await?;
    Ok(())
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}
