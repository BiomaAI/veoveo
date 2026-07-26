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
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    ServerSlug, TelemetryGuard, TokenIssuer, init_server_telemetry, public_allowed_hosts,
};

use crate::{
    mcp::{SimulationViewMcp, SimulationViewMcpState},
    runtime::RuntimeClients,
    state::SimulationViewService,
};

use auth::{InternalAuthState, authenticate_internal};
use config::Args;
use host::validate_host;
use signaling::SignalingState;

pub(crate) const SERVER_SLUG: &str = "simulation-view";

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
    let runtimes = Arc::new(RuntimeClients::new(
        &args.renderer_endpoint,
        &args.pose_endpoint,
        &args.renderer_control_token,
        &args.pose_control_token,
        &args.renderer_signaling_url,
        args.public_media_port,
    )?);
    let mcp_state = SimulationViewMcpState::new(
        service.clone(),
        runtimes.clone(),
        &args.public_signaling_url,
    )?;
    let signaling = SignalingState::new(
        service,
        mcp_state.clone(),
        &args.renderer_signaling_url,
        args.public_media_port,
    )?;

    let cancellation = tokio_util::sync::CancellationToken::new();
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
    let service_router = Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route(
            "/readyz",
            get(move || {
                let readiness = readiness.clone();
                async move {
                    let report = readiness.readiness().await;
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
