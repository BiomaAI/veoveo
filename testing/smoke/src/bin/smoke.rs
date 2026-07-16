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
    GatewayInternalTrustBundle, GatewayTaskStatusDocument, GatewayTaskStatusKind,
    RELATED_TASK_META_KEY,
};

#[path = "smoke/scenarios.rs"]
mod scenarios;
#[path = "smoke/support.rs"]
mod support;

use scenarios::*;

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
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
    /// Smoke-test Helm and k3d local deployment rendering.
    HelmConfig,
    /// Apply Bioma's typed ConfigMap and Secret resources to its isolated cluster.
    BiomaResources {
        /// Kubernetes context owned by the Bioma k3d cluster.
        #[arg(long, default_value = "k3d-veoveo-bioma")]
        context: String,
    },
    /// Verify the Bioma installation and its public Cloudflare edge.
    BiomaVerify {
        /// Kubernetes context owned by the Bioma k3d cluster.
        #[arg(long, default_value = "k3d-veoveo-bioma")]
        context: String,
        /// Loopback origin projected by the Bioma k3d load balancer.
        #[arg(long, default_value = "http://127.0.0.1:8781")]
        local_base_url: String,
        /// Public Cloudflare hostname for the Bioma installation.
        #[arg(long, default_value = "https://veoveo.bioma.ai")]
        public_base_url: String,
        /// Public Cloudflare hostname for the Bioma object store.
        #[arg(long, default_value = "https://objects-veoveo.bioma.ai")]
        object_base_url: String,
    },
    /// Run every live SurrealDB integration target against an isolated 3.2.1 container.
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
    /// Smoke-test direct hosted frame tools, tasks, artifacts, and usage.
    FramesMcp {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built frames MCP server binary path.
        #[arg(long, default_value = "target/debug/server")]
        frames_bin: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
    },
    /// Smoke-test the all-in-one Map image, governed acquisition, activation, and MCP data surface.
    MapMcp {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long, default_value = "target/debug/artifact-service")]
        artifact_service_bin: PathBuf,
        /// Map container image containing DuckDB Spatial, GDAL, the acquisition helper, and Valhalla.
        #[arg(long, default_value = "veoveo/map-mcp:0.1.0")]
        map_image: String,
    },
    /// Smoke-test the production View MCP image through NVIDIA, MCP tasks, and frame resources.
    ViewMcp {
        /// Production View MCP container image.
        #[arg(long, default_value = "veoveo/view-mcp:0.1.0")]
        view_image: String,
        /// Optional path that retains the deterministic rendered frame.
        #[arg(long)]
        retained_frame: Option<PathBuf>,
    },
    /// Run billed live Google 3D Tiles acceptance through the production View MCP boundary.
    ViewGoogleLive {
        /// Production View MCP container image.
        #[arg(long, default_value = "veoveo/view-mcp:0.1.0")]
        view_image: String,
        /// Path for the retained Statue of Liberty JPEG.
        #[arg(long, default_value = "/tmp/veoveo-view-proof/statue-of-liberty.jpg")]
        output: PathBuf,
    },
    /// Smoke-test the Python datasheet template server end to end.
    DatasheetMcp {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
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
    /// Prove producer discovery, OAuth, gateway policy, and Hub durability end to end.
    RecordingIngest {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built gateway binary path.
        #[arg(long, default_value = "target/debug/gateway")]
        gateway_bin: PathBuf,
        /// Built Recording Hub spooler binary path.
        #[arg(long, default_value = "target/debug/spooler")]
        hub_bin: PathBuf,
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
    /// Smoke-test the live console SSE stream (cursor, replay, limits).
    GatewayConsoleStream {
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
    /// Smoke-test the Pilot agent's full mission loop over frames and optimization.
    AgentPilot {
        /// Built conformance binary path.
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Built frames MCP server binary path.
        #[arg(long, default_value = "target/debug/frames-mcp-smoke")]
        frames_bin: PathBuf,
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
        /// Kubernetes context owned by the SUMO development cluster.
        #[arg(long, default_value = "k3d-veoveo-sumo")]
        context: String,
    },
    /// Run the DeepStream GPU detector through Recording Hub and the final MCP task protocol.
    PerceptionGpu {
        /// Environment file used by the active k3d profile and direct assertion signer.
        #[arg(long, default_value = ".env")]
        env_file: PathBuf,
        /// Host workspace for the generated DeepStream sample.
        #[arg(long, default_value = "output/perception/work")]
        work_dir: PathBuf,
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
        Cmd::HelmConfig => helm_config().await,
        Cmd::BiomaResources { context } => bioma_resources(&context),
        Cmd::BiomaVerify {
            context,
            local_base_url,
            public_base_url,
            object_base_url,
        } => {
            bioma_verify(
                &context,
                &local_base_url,
                &public_base_url,
                &object_base_url,
            )
            .await
        }
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
        Cmd::FramesMcp {
            conformance_bin,
            frames_bin,
            artifact_service_bin,
        } => frames_mcp(&conformance_bin, &frames_bin, &artifact_service_bin).await,
        Cmd::MapMcp {
            conformance_bin,
            artifact_service_bin,
            map_image,
        } => map_mcp(&conformance_bin, &artifact_service_bin, &map_image).await,
        Cmd::ViewMcp {
            view_image,
            retained_frame,
        } => view_mcp(&view_image, retained_frame.as_deref()).await,
        Cmd::ViewGoogleLive { view_image, output } => view_google_live(&view_image, &output).await,
        Cmd::DatasheetMcp {
            conformance_bin,
            artifact_service_bin,
        } => datasheet_mcp(&conformance_bin, &artifact_service_bin).await,
        Cmd::GatewayHttp {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => gateway_http(&conformance_bin, &gateway_bin, &control_plane).await,
        Cmd::RecordingIngest {
            conformance_bin,
            gateway_bin,
            hub_bin,
            control_plane,
        } => recording_ingest(&conformance_bin, &gateway_bin, &hub_bin, &control_plane).await,
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
        Cmd::GatewayConsoleStream {
            conformance_bin,
            gateway_bin,
            control_plane,
        } => gateway_console_stream(&conformance_bin, &gateway_bin, &control_plane).await,
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
            frames_bin,
            optimization_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
            agent_bin,
        } => {
            agent_pilot_mission(
                &conformance_bin,
                &frames_bin,
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
        Cmd::SumoVerify {
            conformance_bin,
            context,
        } => sumo_verify(&conformance_bin, &context).await,
        Cmd::PerceptionGpu { env_file, work_dir } => perception_gpu(&env_file, &work_dir).await,
    }
}
