use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path as AxumPath, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_TYPE, WWW_AUTHENTICATE},
    },
    response::IntoResponse,
    routing::{any, get},
};
use clap::{Parser, Subcommand};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayProfileId, PublicDeployment};
use veoveo_mcp_gateway::{GatewayCatalog, www_authenticate_challenge};

#[derive(Parser, Debug)]
#[command(name = "gateway", about = "Veoveo MCP gateway")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Validate typed gateway control data and exit.
    Validate {
        /// JSON control plane file.
        #[arg(long)]
        control_plane: PathBuf,
    },
    /// Start the gateway process.
    Serve {
        /// Port to bind on 0.0.0.0.
        #[arg(long, default_value_t = 8788)]
        port: u16,
        /// Public base URL for metadata and authorization challenges.
        #[arg(long)]
        public_base_url: String,
        /// JSON control plane file.
        #[arg(long)]
        control_plane: PathBuf,
    },
}

#[derive(Clone)]
struct AppState {
    catalog: Arc<GatewayCatalog>,
    public_base_url: String,
}

#[derive(Debug, Serialize)]
struct Readiness {
    status: &'static str,
    servers: usize,
    profiles: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,gateway=debug".into()),
        )
        .init();

    match Args::parse().command {
        Command::Validate { control_plane } => {
            let catalog = GatewayCatalog::load_json(&control_plane)?;
            println!(
                "ok: {} server(s), {} profile(s)",
                catalog.server_count(),
                catalog.profile_count()
            );
            Ok(())
        }
        Command::Serve {
            port,
            public_base_url,
            control_plane,
        } => serve(port, public_base_url, control_plane).await,
    }
}

async fn serve(port: u16, public_base_url: String, control_plane: PathBuf) -> anyhow::Result<()> {
    let catalog = Arc::new(GatewayCatalog::load_json(&control_plane)?);
    let deployment = PublicDeployment::new(public_base_url)?;
    let ct = CancellationToken::new();
    let state = AppState {
        catalog: catalog.clone(),
        public_base_url: deployment.base_url().to_string(),
    };
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .route(
            "/.well-known/oauth-protected-resource/mcp/{profile}",
            get(protected_resource_metadata),
        )
        .route("/mcp/{profile}", any(mcp_requires_authorization))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(
        "veoveo-mcp-gateway listening on http://{addr} with {} server(s), {} profile(s)",
        catalog.server_count(),
        catalog.profile_count()
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}

async fn readyz(State(state): State<AppState>) -> Json<Readiness> {
    Json(Readiness {
        status: "ready",
        servers: state.catalog.server_count(),
        profiles: state.catalog.profile_count(),
    })
}

async fn protected_resource_metadata(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
) -> impl IntoResponse {
    let Ok(profile_id) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match state.catalog.protected_resource_metadata(&profile_id) {
        Ok(metadata) => {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            (StatusCode::OK, headers, Json(metadata)).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn mcp_requires_authorization(
    State(state): State<AppState>,
    AxumPath(profile): AxumPath<String>,
) -> impl IntoResponse {
    let Ok(profile_id) = GatewayProfileId::new(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(profile) = state.catalog.profile(&profile_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let metadata_url = format!(
        "{}/.well-known/oauth-protected-resource/mcp/{}",
        state.public_base_url, profile.id
    );
    let challenge = www_authenticate_challenge(&metadata_url, &profile.required_scopes);
    let Ok(challenge) = HeaderValue::from_str(&challenge) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let mut headers = HeaderMap::new();
    headers.insert(WWW_AUTHENTICATE, challenge);
    (
        StatusCode::UNAUTHORIZED,
        headers,
        "authorization required for gateway profile",
    )
        .into_response()
}
