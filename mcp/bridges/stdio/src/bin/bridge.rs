//! Veoveo stdio-to-streamable-HTTP MCP bridge.
//!
//! Spawns a stdio MCP server as a child process and re-exposes its tool
//! surface over MCP streamable HTTP, so the gateway can register it as a
//! standard `streamable_http` upstream. The bridge forwards tools only; it
//! does not add auth, so it must stay on loopback or an internal network.

#[path = "bridge/handler.rs"]
mod handler;

use std::net::SocketAddr;

use anyhow::{Context, bail};
use axum::{Router, routing::get};
use clap::Parser;
use rmcp::{
    ServiceExt,
    transport::{
        TokioChildProcess,
        streamable_http_server::{StreamableHttpService, session::local::LocalSessionManager},
    },
};
use tokio::process::Command;
use veoveo_mcp_contract::{TelemetryGuard, init_server_telemetry};

use handler::BridgeMcp;

/// Expose a stdio MCP server as an MCP streamable HTTP endpoint.
#[derive(Debug, Parser)]
struct Args {
    /// Socket address the HTTP endpoint listens on.
    #[arg(long, env = "BRIDGE_LISTEN")]
    listen: SocketAddr,
    /// HTTP path serving the MCP endpoint.
    #[arg(long, env = "BRIDGE_MCP_PATH", default_value = "/mcp")]
    mcp_path: String,
    /// Host header values accepted by the HTTP endpoint.
    #[arg(
        long = "allowed-host",
        env = "BRIDGE_ALLOWED_HOSTS",
        value_delimiter = ','
    )]
    allowed_hosts: Vec<String>,
    /// Stdio MCP server command and arguments, e.g. `-- rerun viewer-mcp`.
    #[arg(trailing_var_arg = true, required = true)]
    child_command: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-mcp-stdio-bridge", "info,bridge=debug")?;
    let args = Args::parse();

    let (program, program_args) = args
        .child_command
        .split_first()
        .context("child command must not be empty")?;
    let mut command = Command::new(program);
    command.args(program_args);
    let transport = TokioChildProcess::new(command)
        .with_context(|| format!("failed to spawn stdio MCP child `{program}`"))?;
    let child = ()
        .serve(transport)
        .await
        .with_context(|| format!("failed to initialize stdio MCP child `{program}`"))?;
    let child_info = child
        .peer_info()
        .context("stdio MCP child returned no server info")?;
    tracing::info!(
        server = %child_info.server_info.name,
        version = %child_info.server_info.version,
        "stdio MCP child initialized"
    );
    let server_info = handler::bridge_server_info(&child_info);
    let peer = child.peer().clone();

    let ct = tokio_util::sync::CancellationToken::new();
    let mut http_config = veoveo_mcp_contract::canonical_streamable_http_server_config()
        .with_cancellation_token(ct.child_token());
    if !args.allowed_hosts.is_empty() {
        http_config = http_config.with_allowed_hosts(args.allowed_hosts.iter().cloned());
    }
    let mcp_service = StreamableHttpService::new(
        move || Ok(BridgeMcp::new(peer.clone(), server_info.clone())),
        LocalSessionManager::default().into(),
        http_config,
    );
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route_service(&args.mcp_path, mcp_service);
    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("failed to bind {}", args.listen))?;
    tracing::info!(
        "bridge listening on http://{}{}",
        args.listen,
        args.mcp_path
    );

    tokio::select! {
        result = axum::serve(listener, router) => {
            ct.cancel();
            result.context("bridge HTTP server failed")?;
            bail!("bridge HTTP server exited unexpectedly");
        }
        reason = child.waiting() => {
            ct.cancel();
            bail!("stdio MCP child exited: {reason:?}");
        }
    }
}
