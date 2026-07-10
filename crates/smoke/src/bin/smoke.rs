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
    header::{CONTENT_TYPE, HOST, LOCATION},
    redirect::Policy,
};
use rmcp::{
    ClientHandler, ServiceExt,
    model::{
        CallToolRequestParams, ClientCapabilities, ClientInfo, Implementation,
        ReadResourceRequestParams, ResourceContents,
    },
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;
use veoveo_mcp_contract::{
    GatewayTaskStatusDocument, GatewayTaskStatusKind, RELATED_TASK_META_KEY,
};

#[path = "smoke/scenarios.rs"]
mod scenarios;
#[path = "smoke/support.rs"]
mod support;

use scenarios::*;

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

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
    /// Run every live SurrealDB integration target against an isolated 3.2.0 container.
    SurrealIntegration,
    /// Smoke-test gateway platform bootstrap and active revision validation.
    GatewayPlatformStore {
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
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
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
    },
    /// Smoke-test direct hosted media task behavior without gateway projection.
    MediaTaskRun {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built media MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        media_bin: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
    },
    /// Smoke-test direct hosted coordinates tools, tasks, artifacts, and usage.
    CoordinatesMcp {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built coordinates MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        coordinates_bin: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
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
    /// Verify browser OAuth against a pinned, real HTTPS Keycloak identity provider.
    GatewayKeycloak {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Keycloak realm import fixture.
        #[arg(long, default_value = "configs/keycloak/veoveo-ci-realm.json")]
        realm: PathBuf,
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
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
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
    /// Smoke-test gateway projection for server-owned chart resources.
    GatewayChartProjection {
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
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
    },
    /// Smoke-test the agent kernel's durable task detach and resume across processes.
    AgentKernel {
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
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
        /// Built agent kernel binary path.
        #[arg(long, default_value = "target/debug/agent")]
        agent_bin: PathBuf,
    },
    /// Smoke-test a continuously-running agent sleeping on a long gateway task and waking from its completion push. --live swaps in the real model from CLOUDFLARE_* env.
    AgentSleepWake {
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
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
        /// Built agent kernel binary path.
        #[arg(long, default_value = "target/debug/agent")]
        agent_bin: PathBuf,
        /// Use the real model from CLOUDFLARE_ACCOUNT_ID/CLOUDFLARE_API_TOKEN
        /// (model id from AGENT_LIVE_MODEL) instead of the scripted fake.
        #[arg(long, default_value_t = false)]
        live: bool,
    },
    /// Smoke-test the Pilot agent's full mission loop over coordinates and optimization.
    AgentPilot {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built coordinates MCP server binary path.
        #[arg(long, default_value = "target/debug/coordinates-mcp-smoke")]
        coordinates_bin: PathBuf,
        /// Built optimization MCP server binary path.
        #[arg(long, default_value = "target/debug/optimization-mcp-smoke")]
        optimization_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
        /// Built agent kernel binary path.
        #[arg(long, default_value = "target/debug/agent")]
        agent_bin: PathBuf,
    },
    /// Smoke-test the agent kernel's scheduler: heartbeats, operator wakes, budgets, fail-closed manifests.
    AgentKernelScheduler {
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
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
        /// Built agent kernel binary path.
        #[arg(long, default_value = "target/debug/agent")]
        agent_bin: PathBuf,
    },
    /// Smoke-test agent-kernel gateway prerequisites: optional-tool task calls and cross-session task continuity.
    AgentGateway {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built duckdb MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        duckdb_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
    },
    /// Smoke-test gateway secret resolution against a real Vault KV v2 service.
    GatewayVaultSecrets {
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    /// Prove typed SUMO world frames survive the Recording Hub durability boundary.
    SumoPush {
        #[arg(long, default_value_t = 40)]
        steps: u32,
    },
    /// Run the real LuST/SUMO container and verify its authenticated MCP and durable recording.
    SumoVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    install_rustls_provider();
    let args = Args::parse();
    match args.cmd {
        Cmd::GatewaySuite {
            control_plane,
            smoke_control_plane,
        } => gateway_suite(&control_plane, &smoke_control_plane).await,
        Cmd::ComposeConfig => compose_config().await,
        Cmd::SurrealIntegration => surreal_integration().await,
        Cmd::GatewayPlatformStore {
            gateway_bin,
            control_plane,
        } => gateway_platform_store(&gateway_bin, &control_plane).await,
        Cmd::ContractSchemas { conformance_bin } => contract_schemas(&conformance_bin),
        Cmd::Otel {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => otel(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::MediaMcpAuth {
            conformance_bin,
            media_bin,
            artifact_service_bin,
        } => media_mcp_auth(&conformance_bin, &media_bin, &artifact_service_bin).await,
        Cmd::MediaTaskRun {
            conformance_bin,
            media_bin,
            artifact_service_bin,
        } => media_task_run(&conformance_bin, &media_bin, &artifact_service_bin).await,
        Cmd::CoordinatesMcp {
            conformance_bin,
            coordinates_bin,
            artifact_service_bin,
        } => coordinates_mcp(&conformance_bin, &coordinates_bin, &artifact_service_bin).await,
        Cmd::GatewayHttp {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => gateway_http(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::GatewayKeycloak {
            conformance_bin,
            gateway_bin,
            control_plane,
            realm,
        } => gateway_keycloak(&conformance_bin, &gateway_bin, &control_plane, &realm).await,
        Cmd::GatewayAuthenticated {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
        } => {
            gateway_authenticated(
                &conformance_bin,
                &media_bin,
                &gateway_bin,
                &control_plane,
                &artifact_service_bin,
            )
            .await
        }
        Cmd::GatewayTwoServers {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => gateway_two_servers(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::GatewayChartProjection {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => gateway_chart_projection(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::GatewayTaskRun {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
        } => {
            gateway_task_run(
                &conformance_bin,
                &media_bin,
                &gateway_bin,
                &control_plane,
                &artifact_service_bin,
            )
            .await
        }
        Cmd::AgentKernel {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
            agent_bin,
        } => {
            agent_kernel_detach_resume(
                &conformance_bin,
                &media_bin,
                &gateway_bin,
                &control_plane,
                &artifact_service_bin,
                &agent_bin,
            )
            .await
        }
        Cmd::AgentSleepWake {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
            agent_bin,
            live,
        } => {
            agent_sleep_wake(
                &conformance_bin,
                &media_bin,
                &gateway_bin,
                &control_plane,
                &artifact_service_bin,
                &agent_bin,
                live,
            )
            .await
        }
        Cmd::AgentPilot {
            conformance_bin,
            coordinates_bin,
            optimization_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
            agent_bin,
        } => {
            agent_pilot_mission(
                &conformance_bin,
                &coordinates_bin,
                &optimization_bin,
                &gateway_bin,
                &control_plane,
                &artifact_service_bin,
                &agent_bin,
            )
            .await
        }
        Cmd::AgentKernelScheduler {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
            agent_bin,
        } => {
            agent_kernel_scheduler(
                &conformance_bin,
                &media_bin,
                &gateway_bin,
                &control_plane,
                &artifact_service_bin,
                &agent_bin,
            )
            .await
        }
        Cmd::AgentGateway {
            conformance_bin,
            duckdb_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
        } => {
            agent_gateway(
                &conformance_bin,
                &duckdb_bin,
                &gateway_bin,
                &control_plane,
                &artifact_service_bin,
            )
            .await
        }
        Cmd::GatewayVaultSecrets {
            gateway_bin,
            control_plane,
        } => gateway_vault_secrets(&gateway_bin, &control_plane).await,
        Cmd::SumoPush { steps } => sumo_push(steps).await,
        Cmd::SumoVerify { conformance_bin } => sumo_verify(&conformance_bin).await,
    }
}
