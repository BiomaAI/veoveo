use std::{
    env,
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use clap::{Parser, Subcommand};
use reqwest::{
    StatusCode,
    header::{CONTENT_TYPE, LOCATION},
    redirect::Policy,
};
use rmcp::{
    ClientHandler, ServiceExt,
    model::{
        ClientCapabilities, ClientInfo, Implementation, ReadResourceRequestParams, ResourceContents,
    },
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

#[path = "smoke/scenarios.rs"]
mod scenarios;
#[path = "smoke/support.rs"]
mod support;

use scenarios::*;

#[derive(Parser, Debug)]
#[command(name = "smoke", about = "Veoveo smoke-test harness")]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run the full production gateway smoke suite.
    GatewaySuite {
        /// Local gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.local.json")]
        control_plane: PathBuf,
        /// Gateway control-plane JSON used by smoke scenarios.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        smoke_control_plane: PathBuf,
    },
    /// Smoke-test Compose edge routing and published-port shape.
    ComposeConfig,
    /// Smoke-test contract schema export for external implementations.
    ContractSchemas {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
    },
    /// Smoke-test OTLP HTTP log and trace export from the gateway.
    Otel {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    /// Smoke-test the media MCP HTTP boundary and internal assertion requirement.
    MediaMcpAuth {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built media MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        media_bin: PathBuf,
    },
    /// Smoke-test direct hosted media task behavior without gateway projection.
    MediaTaskRun {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built media MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        media_bin: PathBuf,
    },
    /// Smoke-test the gateway HTTP boundary, auth discovery, and browser OAuth flow.
    GatewayHttp {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    /// Smoke-test authenticated gateway-to-media forwarding and policy/admin flows.
    GatewayAuthenticated {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built media MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        media_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    /// Run one gateway profile against two hosted MCP upstreams.
    GatewayTwoServers {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    /// Smoke-test a full gateway task run with webhook completion and usage.
    GatewayTaskRun {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built media MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        media_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    match args.cmd {
        Cmd::GatewaySuite {
            control_plane,
            smoke_control_plane,
        } => gateway_suite(&control_plane, &smoke_control_plane).await,
        Cmd::ComposeConfig => compose_config().await,
        Cmd::ContractSchemas { conformance_bin } => contract_schemas(&conformance_bin),
        Cmd::Otel {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => otel(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::MediaMcpAuth {
            conformance_bin,
            media_bin,
        } => media_mcp_auth(&conformance_bin, &media_bin).await,
        Cmd::MediaTaskRun {
            conformance_bin,
            media_bin,
        } => media_task_run(&conformance_bin, &media_bin).await,
        Cmd::GatewayHttp {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => gateway_http(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::GatewayAuthenticated {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
        } => {
            gateway_authenticated(&conformance_bin, &media_bin, &gateway_bin, &control_plane).await
        }
        Cmd::GatewayTwoServers {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => gateway_two_servers(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::GatewayTaskRun {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
        } => gateway_task_run(&conformance_bin, &media_bin, &gateway_bin, &control_plane).await,
    }
}
