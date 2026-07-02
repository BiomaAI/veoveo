use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{Json, Router, extract::State, routing::get};
use clap::{Parser, Subcommand};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_gateway::GatewayCatalog;

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
        /// JSON control plane file.
        #[arg(long)]
        control_plane: PathBuf,
    },
}

#[derive(Clone)]
struct AppState {
    catalog: Arc<GatewayCatalog>,
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
            control_plane,
        } => serve(port, control_plane).await,
    }
}

async fn serve(port: u16, control_plane: PathBuf) -> anyhow::Result<()> {
    let catalog = Arc::new(GatewayCatalog::load_json(&control_plane)?);
    let ct = CancellationToken::new();
    let state = AppState {
        catalog: catalog.clone(),
    };
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
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
