mod proxy;

use std::{env, net::SocketAddr};

use anyhow::{Context, bail, ensure};
use axum::{Router, routing::get};
use clap::{Parser, Subcommand};
use rmcp::{
    ClientHandler, ClientLifecycleMode, ClientServiceExt,
    model::{ClientCapabilities, ClientInfo, Implementation, ProtocolVersion},
    service::{Peer, RoleClient},
    transport::{
        StreamableHttpClientTransport, TokioChildProcess,
        streamable_http_client::StreamableHttpClientTransportConfig,
        streamable_http_server::StreamableHttpService,
    },
};
use tokio::{process::Command, task::JoinHandle};
use veoveo_mcp_contract::{TelemetryGuard, init_server_telemetry};

use proxy::{LegacyProxy, final_server_info};

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, env = "LEGACY_BRIDGE_LISTEN")]
    listen: SocketAddr,
    #[arg(long, env = "LEGACY_BRIDGE_MCP_PATH", default_value = "/mcp")]
    mcp_path: String,
    #[arg(
        long = "allowed-host",
        env = "LEGACY_BRIDGE_ALLOWED_HOSTS",
        value_delimiter = ','
    )]
    allowed_hosts: Vec<String>,
    #[command(subcommand)]
    connector: Connector,
}

#[derive(Debug, Subcommand)]
enum Connector {
    /// Own one explicitly configured local legacy stdio child.
    Stdio {
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
    /// Own one explicitly configured remote legacy Streamable HTTP connection.
    Http {
        #[arg(long)]
        url: String,
        /// Environment variable containing the optional bearer token.
        #[arg(long)]
        bearer_token_env: Option<String>,
    },
}

#[derive(Clone, Copy)]
struct LegacyClient;

impl ClientHandler for LegacyClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::new(
            ClientCapabilities::default(),
            Implementation::new("veoveo-legacy-bridge", env!("CARGO_PKG_VERSION")),
        )
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-mcp-legacy-bridge", "info,legacy_bridge=debug")?;
    let args = Args::parse();
    let (legacy, owner) = connect(args.connector).await?;
    let observed = legacy
        .peer_info()
        .context("legacy server returned no Initialize result")?;
    ensure!(
        observed.protocol_version == ProtocolVersion::V_2025_11_25,
        "legacy connector requires MCP 2025-11-25, received {}",
        observed.protocol_version
    );
    let info = final_server_info(&observed);
    let peer = legacy.clone();

    let cancellation = tokio_util::sync::CancellationToken::new();
    let mut config = veoveo_mcp_contract::canonical_streamable_http_server_config()
        .with_cancellation_token(cancellation.child_token());
    if !args.allowed_hosts.is_empty() {
        config = config.with_allowed_hosts(args.allowed_hosts.iter().cloned());
    }
    let service = StreamableHttpService::new(
        move || Ok(LegacyProxy::new(peer.clone(), info.clone())),
        veoveo_mcp_contract::stateless_session_manager(),
        config,
    );
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route_service(&args.mcp_path, service);
    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("failed to bind {}", args.listen))?;
    tracing::info!(listen = %args.listen, path = %args.mcp_path, "legacy bridge ready");

    tokio::select! {
        result = axum::serve(listener, router) => {
            cancellation.cancel();
            result.context("legacy bridge HTTP server failed")?;
            bail!("legacy bridge HTTP server exited unexpectedly");
        }
        result = owner => {
            cancellation.cancel();
            bail!("legacy connector exited: {}", result.context("connector owner task panicked")?);
        }
    }
}

async fn connect(connector: Connector) -> anyhow::Result<(Peer<RoleClient>, JoinHandle<String>)> {
    match connector {
        Connector::Stdio { command } => {
            let (program, arguments) = command
                .split_first()
                .context("legacy stdio command must not be empty")?;
            let mut child = Command::new(program);
            child.args(arguments);
            let transport = TokioChildProcess::new(child)
                .with_context(|| format!("failed to spawn legacy child `{program}`"))?;
            let running = LegacyClient
                .serve_with_lifecycle(transport, ClientLifecycleMode::Initialize)
                .await
                .with_context(|| format!("failed to Initialize legacy child `{program}`"))?;
            owned_peer(running)
        }
        Connector::Http {
            url,
            bearer_token_env,
        } => {
            let mut config = StreamableHttpClientTransportConfig::with_uri(url.clone());
            if let Some(name) = bearer_token_env {
                let token = env::var(&name).with_context(|| {
                    format!("legacy bearer-token environment variable `{name}` is unset")
                })?;
                ensure!(!token.trim().is_empty(), "legacy bearer token is empty");
                config = config.auth_header(token);
            }
            let running = LegacyClient
                .serve_with_lifecycle(
                    StreamableHttpClientTransport::from_config(config),
                    ClientLifecycleMode::Initialize,
                )
                .await
                .with_context(|| format!("failed to Initialize legacy HTTP server `{url}`"))?;
            owned_peer(running)
        }
    }
}

fn owned_peer(
    running: rmcp::service::RunningService<RoleClient, LegacyClient>,
) -> anyhow::Result<(Peer<RoleClient>, JoinHandle<String>)> {
    let peer = running.peer().clone();
    let owner = tokio::spawn(async move { format!("{:?}", running.waiting().await) });
    Ok((peer, owner))
}
