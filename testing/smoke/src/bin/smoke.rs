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
use clap::{Parser, Subcommand, ValueEnum};
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
use veoveo_extension_contract::SimulationOverlayKind;
use veoveo_mcp_contract::{
    GatewayTaskStatusDocument, GatewayTaskStatusKind, RELATED_TASK_META_KEY,
};

#[path = "smoke/deployment.rs"]
mod deployment;
#[path = "smoke/scenarios.rs"]
mod scenarios;
#[path = "smoke/support.rs"]
mod support;

use deployment::*;
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
    /// Build and test the external simulation fixture from an authenticated published SDK wheel.
    ExternalSimulationFixture,
    /// Validate one typed deployment profile and every selected build and Helm surface.
    ProfileValidate {
        /// Deployment profile JSON.
        #[arg(long)]
        profile: PathBuf,
    },
    /// Start the standalone local registry selected by a deployment profile.
    ProfileRegistryUp {
        /// Deployment profile JSON.
        #[arg(long)]
        profile: PathBuf,
    },
    /// Create or start the local k3d cluster selected by a deployment profile.
    ProfileClusterUp {
        /// Deployment profile JSON.
        #[arg(long)]
        profile: PathBuf,
    },
    /// Stop the local k3d cluster selected by a deployment profile.
    ProfileClusterStop {
        /// Deployment profile JSON.
        #[arg(long)]
        profile: PathBuf,
    },
    /// Delete the local k3d cluster selected by a deployment profile.
    ProfileClusterDelete {
        /// Deployment profile JSON.
        #[arg(long)]
        profile: PathBuf,
    },
    /// Apply a profile's resources and independently resolved Helm releases.
    ProfileUp {
        /// Deployment profile JSON.
        #[arg(long)]
        profile: PathBuf,
        /// Immutable deployment lock produced by `cargo xtask release images --profile`.
        #[arg(long)]
        lock: PathBuf,
    },
    /// Uninstall every Helm release selected by a deployment profile.
    ProfileDown {
        /// Deployment profile JSON.
        #[arg(long)]
        profile: PathBuf,
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
        #[arg(long, default_value = "target/debug/media-mcp")]
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
        #[arg(long, default_value = "target/debug/media-mcp")]
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
        #[arg(long, default_value = "target/debug/frames-mcp")]
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
        #[arg(long, default_value = "target/debug/media-mcp")]
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
        #[arg(long, default_value = "target/debug/media-mcp")]
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
        #[arg(long, default_value = "target/debug/media-mcp")]
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
        #[arg(long, default_value = "target/debug/media-mcp")]
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
        #[arg(long, default_value = "target/debug/frames-mcp")]
        frames_bin: PathBuf,
        /// Built optimization MCP server binary path.
        #[arg(long, default_value = "target/debug/optimization-mcp")]
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
        #[arg(long, default_value = "target/debug/media-mcp")]
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
        #[arg(long, default_value = "target/debug/duckdb-mcp")]
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
    /// Verify the independent UAV domain path through flight, live Stream, recording replay, and Reason.
    UavDomainVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Runtime-loaded mission and acceptance parameters.
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        /// Kubernetes context containing the UAV showcase.
        #[arg(long)]
        context: String,
        /// Public installation base URL used for OAuth and MCP.
        #[arg(long)]
        public_base_url: String,
    },
    /// Run UAV flight and prove its independent Simulation View follow camera in the real Console.
    UavShowcaseVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        /// Kubernetes context containing the composed showcase.
        #[arg(long)]
        context: String,
        /// Namespace containing the platform and showcase releases.
        #[arg(long, default_value = "veoveo")]
        namespace: String,
        /// Public installation base URL used by MCP and the authenticated Console.
        #[arg(long)]
        public_base_url: String,
        /// HTTP discovery or direct ws:// browser endpoint for headed hardware-backed Chrome.
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        /// Root for revision- and run-qualified JSON and PNG evidence.
        #[arg(long, default_value = "output/acceptance/uav")]
        evidence_root: PathBuf,
    },
    /// Verify the independent Simulation View renderer with an anonymous pose producer and headed hardware browser.
    SimulationViewVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        /// Kubernetes context containing the composed platform and external fixture.
        #[arg(long)]
        context: String,
        /// Namespace containing Simulation View and the anonymous fixture.
        #[arg(long, default_value = "veoveo")]
        namespace: String,
        /// Public gateway and signaling origin.
        #[arg(long)]
        public_base_url: String,
        /// Work Context selected for the automated operator identity.
        #[arg(long, default_value = "operations")]
        work_context: String,
        /// HTTP discovery or direct ws:// browser endpoint for headed hardware-backed Chrome.
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        /// Directory that receives immutable per-run visual evidence.
        #[arg(long, default_value = "output/acceptance/simulation-view")]
        evidence_root: PathBuf,
        /// Maximum time for Isaac, poses, render products, NVENC, WebRTC, and video playback.
        #[arg(long, default_value_t = 300)]
        timeout_seconds: u64,
    },
    /// Certify an immutable simulation overlay and base on NVIDIA hardware.
    SimulationCertify {
        /// Canonical base image using repository@sha256 identity.
        #[arg(long)]
        base_image: String,
        /// Simulator overlay image using repository@sha256 identity.
        #[arg(long)]
        overlay_image: String,
        /// Supported overlay class.
        #[arg(long, value_enum)]
        overlay_kind: SimulationOverlayArg,
        /// Full source revision that produced the overlay.
        #[arg(long)]
        source_revision: String,
        /// Machine-readable hardware result.
        #[arg(long)]
        output: PathBuf,
        /// Persistent host shader and kernel cache.
        #[arg(long, default_value = "output/simulation-certification/runtime-cache")]
        cache_directory: PathBuf,
        /// Hard upper bound including an uncached first Kit launch.
        #[arg(long, default_value_t = 1200)]
        timeout_seconds: u64,
    },
    /// Run the DeepStream GPU detector through Recording Hub and the final MCP task protocol.
    StreamGpu {
        /// Environment file used by the active k3d profile and direct assertion signer.
        #[arg(long, default_value = ".env")]
        env_file: PathBuf,
        /// Host workspace for the generated DeepStream sample.
        #[arg(long, default_value = "output/stream/work")]
        work_dir: PathBuf,
    },
    /// Run the world-model GPU reasoner through Recording Hub and the final MCP task protocol.
    ReasonGpu {
        /// Environment file used by the active k3d profile and direct assertion signer.
        #[arg(long, default_value = ".env")]
        env_file: PathBuf,
        /// Host workspace for the generated DeepStream sample.
        #[arg(long, default_value = "output/reason/work")]
        work_dir: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SimulationOverlayArg {
    FirstPartyUav,
    AnonymousExternal,
}

impl From<SimulationOverlayArg> for SimulationOverlayKind {
    fn from(value: SimulationOverlayArg) -> Self {
        match value {
            SimulationOverlayArg::FirstPartyUav => Self::FirstPartyUav,
            SimulationOverlayArg::AnonymousExternal => Self::AnonymousExternal,
        }
    }
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
        Cmd::ExternalSimulationFixture => external_simulation_fixture(),
        Cmd::ProfileValidate { profile } => profile_validate(&profile),
        Cmd::ProfileRegistryUp { profile } => profile_registry_up(&profile),
        Cmd::ProfileClusterUp { profile } => profile_cluster_up(&profile),
        Cmd::ProfileClusterStop { profile } => profile_cluster_stop(&profile),
        Cmd::ProfileClusterDelete { profile } => profile_cluster_delete(&profile),
        Cmd::ProfileUp { profile, lock } => profile_up(&profile, &lock),
        Cmd::ProfileDown { profile } => profile_down(&profile),
        Cmd::BiomaVerify {
            context,
            local_base_url,
            public_base_url,
        } => bioma_verify(&context, &local_base_url, &public_base_url).await,
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
        Cmd::UavDomainVerify {
            conformance_bin,
            scenario,
            context,
            public_base_url,
        } => uav_sim_verify(&conformance_bin, &scenario, &context, &public_base_url).await,
        Cmd::UavShowcaseVerify {
            conformance_bin,
            scenario,
            context,
            namespace,
            public_base_url,
            chrome_cdp_url,
            evidence_root,
        } => {
            uav_showcase_verify(
                &conformance_bin,
                &scenario,
                &context,
                &namespace,
                &public_base_url,
                &chrome_cdp_url,
                &evidence_root,
            )
            .await
        }
        Cmd::SimulationViewVerify {
            conformance_bin,
            context,
            namespace,
            public_base_url,
            work_context,
            chrome_cdp_url,
            evidence_root,
            timeout_seconds,
        } => {
            simulation_view_verify(SimulationViewVerifyRequest {
                conformance: &conformance_bin,
                context: &context,
                namespace: &namespace,
                public_base_url: &public_base_url,
                work_context: &work_context,
                chrome_cdp_url: &chrome_cdp_url,
                evidence_root: &evidence_root,
                timeout: Duration::from_secs(timeout_seconds),
            })
            .await
        }
        Cmd::SimulationCertify {
            base_image,
            overlay_image,
            overlay_kind,
            source_revision,
            output,
            cache_directory,
            timeout_seconds,
        } => {
            simulation_certify(
                &base_image,
                &overlay_image,
                overlay_kind.into(),
                &source_revision,
                &output,
                &cache_directory,
                Duration::from_secs(timeout_seconds),
            )
            .await
        }
        Cmd::StreamGpu { env_file, work_dir } => stream_gpu(&env_file, &work_dir).await,
        Cmd::ReasonGpu { env_file, work_dir } => reason_gpu(&env_file, &work_dir).await,
    }
}
