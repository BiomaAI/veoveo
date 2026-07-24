//! Streamable HTTP entrypoint for the dedicated artifact MCP server.

use std::{net::SocketAddr, sync::Arc};

use axum::{Router, middleware, routing::get};
use clap::Parser;
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, session::local::LocalSessionManager,
};
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    ServerSlug, TelemetryGuard, TokenIssuer, init_server_telemetry, public_allowed_hosts,
};
use veoveo_platform_store::PlatformStore;

#[path = "server/auth.rs"]
mod auth;
#[path = "server/config.rs"]
mod config;
#[path = "server/handler.rs"]
mod handler;
#[path = "server/host.rs"]
mod host;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/subscriptions.rs"]
mod subscriptions;

use auth::InternalAuthState;
use config::Args;
use handler::{AppState, ArtifactMcp};
use subscriptions::{ArtifactSubscriptions, start_dispatcher};

const SERVER_SLUG: &str = "artifact";

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-artifact-mcp", "info,veoveo_artifact_mcp=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let store = PlatformStore::connect(args.store_config()?).await?;
    let plane = HttpArtifactPlane::new(args.artifact_service_url);
    let subscriptions = ArtifactSubscriptions::default();
    let state = Arc::new(AppState {
        plane: plane.clone(),
        subscriptions: subscriptions.clone(),
        public_base_url: public_deployment
            .base_url()
            .trim_end_matches('/')
            .to_owned(),
    });
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let cancellation = CancellationToken::new();
    start_dispatcher(store, plane, subscriptions, cancellation.child_token()).await?;

    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts);
    let allowed_hosts = Arc::new(allowed_hosts);
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(ArtifactMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(cancellation.child_token()),
    );
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            InternalAuthState { verifier },
            auth::authenticate,
        ));
    let server_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .nest("/mcp", mcp_router);
    let router = Router::new()
        .nest(public_endpoint.mount_path(), server_router)
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            host::validate,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(
        service = "veoveo-artifact-mcp",
        %address,
        mcp_path = public_endpoint.path("mcp"),
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
