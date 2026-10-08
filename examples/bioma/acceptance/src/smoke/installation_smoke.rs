#[path = "scenarios/candidate.rs"]
mod candidate;
use anyhow::Context;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use base64::Engine as _;
use clap::Parser;
use clap::Subcommand;
use reqwest::StatusCode;
use reqwest::header::HOST;
use reqwest::header::LOCATION;
use reqwest::redirect::Policy;
use rmcp::model::ReadResourceRequestParams;
use rmcp::model::ResourceContents;
use serde::Deserialize;
use serde_json::Value;
use veoveo_artifact_contract::parse_artifact_plane_uri;

use veoveo_simulation_contract::SimulationOverlayKind;

use veoveo_duckdb_runtime::smoke::*;
use veoveo_gateway_composition::smoke_support as support;
use veoveo_gateway_composition::smoke_support::*;
use veoveo_testing_support::*;
#[derive(Parser, Debug)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}
#[derive(Subcommand, Debug)]
enum Cmd {
    SurrealIntegration,
    InstallationVerify {
        /// Built conformance binary used as the public machine OAuth and MCP client.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Installation-owned target and public control-plane coordinates.
        #[arg(long)]
        installation: PathBuf,
    },
    ArtifactUploadConsumers {
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        #[arg(long)]
        installation: PathBuf,
        #[arg(long)]
        browser_evidence: PathBuf,
        #[arg(long)]
        evidence_output: PathBuf,
    },
    MediaMcpAuth {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built media MCP server binary path.
        #[arg(long)]
        media_bin: Option<PathBuf>,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
    },
    MediaTaskRun {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built media MCP server binary path.
        #[arg(long)]
        media_bin: Option<PathBuf>,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
    },
    FramesMcp {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built frames MCP server binary path.
        #[arg(long)]
        frames_bin: Option<PathBuf>,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
        /// Installed normal-OAuth target; paired with an evidence output.
        #[arg(long, requires = "evidence_output")]
        installation: Option<PathBuf>,
        #[arg(long, requires = "installation")]
        evidence_output: Option<PathBuf>,
    },
    MapMcp {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
        /// Map container image containing DuckDB Spatial, GDAL, the acquisition helper, and Valhalla.
        #[arg(long, default_value = "veoveo/map-mcp:0.1.0")]
        map_image: String,
    },
    ViewMcp {
        /// Production View MCP container image.
        #[arg(long, default_value = "veoveo/view-mcp:0.1.0")]
        view_image: String,
        /// Optional path that retains the deterministic rendered frame.
        #[arg(long)]
        retained_frame: Option<PathBuf>,
        /// Installed target; explicit deterministic fixture admission is required.
        #[arg(long, requires_all=["installed_fixture","evidence_output"])]
        installation: Option<PathBuf>,
        #[arg(long, requires = "installation")]
        installed_fixture: Option<PathBuf>,
        #[arg(long, requires = "installation")]
        evidence_output: Option<PathBuf>,
    },
    /// Prepare local fixture files; this command is not a smoke qualification.
    ViewFixtureExport {
        #[arg(long)]
        output: PathBuf,
    },
    ViewGoogleLive {
        /// Production View MCP container image.
        #[arg(long, default_value = "veoveo/view-mcp:0.1.0")]
        view_image: String,
        /// Path for the retained Statue of Liberty JPEG.
        #[arg(long, default_value = "/tmp/veoveo-view-proof/statue-of-liberty.jpg")]
        output: PathBuf,
    },
    DatasheetMcp {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
    },
    RecordingIngest {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Built Recording Hub spooler binary path.
        #[arg(long)]
        hub_bin: Option<PathBuf>,
        /// Base gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
    },
    AgentKernel {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built media MCP server binary path.
        #[arg(long)]
        media_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
        /// Built agent kernel binary path.
        #[arg(long)]
        agent_bin: Option<PathBuf>,
    },
    AgentSleepWake {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built media MCP server binary path.
        #[arg(long)]
        media_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
        /// Built agent kernel binary path.
        #[arg(long)]
        agent_bin: Option<PathBuf>,
        /// Use the real model from CLOUDFLARE_ACCOUNT_ID/CLOUDFLARE_API_TOKEN
        /// (model id from AGENT_LIVE_MODEL) instead of the scripted fake.
        #[arg(long, default_value_t = false)]
        live: bool,
    },
    AgentPilot {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built frames MCP server binary path.
        #[arg(long)]
        frames_bin: Option<PathBuf>,
        /// Built optimization MCP server binary path.
        #[arg(long)]
        optimization_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
        /// Built agent kernel binary path.
        #[arg(long)]
        agent_bin: Option<PathBuf>,
    },
    AgentKernelScheduler {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built media MCP server binary path.
        #[arg(long)]
        media_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
        /// Built agent kernel binary path.
        #[arg(long)]
        agent_bin: Option<PathBuf>,
    },
    AgentGateway {
        /// Built conformance binary path.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Built duckdb MCP server binary path.
        #[arg(long)]
        duckdb_bin: Option<PathBuf>,
        /// Built gateway binary path.
        #[arg(long)]
        gateway_bin: Option<PathBuf>,
        /// Gateway control-plane JSON.
        #[arg(long, default_value = "configs/gateway.smoke.json")]
        control_plane: PathBuf,
        /// Built artifact-service binary path.
        #[arg(long)]
        artifact_service_bin: Option<PathBuf>,
    },
    SumoPush {
        #[arg(long, default_value_t = 40)]
        steps: u32,
    },
    SumoVerify {
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Kubernetes context owned by the SUMO development cluster.
        #[arg(long, default_value = "k3d-veoveo-sumo")]
        context: String,
    },
    SimulationCertify {
        /// Validated deployment lock authorizing the registry identity and transport.
        ///
        /// When omitted, certification permits TLS registries only.
        #[arg(long)]
        deployment_lock: Option<PathBuf>,
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
    StreamCompilerStartup {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long)]
        candidate_binary: PathBuf,
        #[arg(long)]
        candidate_app: PathBuf,
        #[arg(long)]
        work_dir: PathBuf,
    },
    StreamGpu {
        #[arg(long)]
        installation: PathBuf,
        /// Environment file used by the active k3d profile and direct assertion signer.
        #[arg(long, default_value = ".env")]
        env_file: PathBuf,
        /// Installed Secret containing the recording producer's private-key.pem.
        #[arg(long)]
        producer_key_secret: String,
        /// Host workspace for the generated DeepStream sample.
        #[arg(long, default_value = "output/stream/work")]
        work_dir: PathBuf,
        /// Candidate executable to qualify in the running NVIDIA container on a separate port.
        #[arg(long, requires = "candidate_app")]
        candidate_binary: Option<PathBuf>,
        /// App HTML packaged with the candidate executable.
        #[arg(long, requires = "candidate_binary")]
        candidate_app: Option<PathBuf>,
        /// Object-detection pipeline admitted by the installation's Stream catalog.
        #[arg(long)]
        pipeline_id: String,
        /// Two ready Stream Pods: dispatch on the first and observe on the second.
        #[arg(long, num_args = 2, value_names = ["WRITER", "OBSERVER"], conflicts_with_all = ["candidate_binary", "candidate_app"])]
        replica_pods: Vec<String>,
    },
    RecordingFixtureFinish {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long, default_value = ".env")]
        env_file: PathBuf,
        #[arg(long)]
        producer_key_secret: String,
        #[arg(long = "stream-id", required = true)]
        stream_ids: Vec<uuid::Uuid>,
    },
    RecordingCatalogSdk {
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        #[arg(long)]
        installation: PathBuf,
        #[arg(long)]
        dataset_id: uuid::Uuid,
        #[arg(long)]
        recording_id: veoveo_recording_contract::RecordingId,
    },
    ReasonGpu {
        #[arg(long)]
        installation: PathBuf,
        /// Environment file used by the active k3d profile and direct assertion signer.
        #[arg(long, default_value = ".env")]
        env_file: PathBuf,
        /// Installed Secret containing the recording producer's private-key.pem.
        #[arg(long)]
        producer_key_secret: String,
        /// Host workspace for the generated DeepStream sample.
        #[arg(long, default_value = "output/reason/work")]
        work_dir: PathBuf,
        /// Candidate server executable, run on a private listener in the installed GPU runtime.
        #[arg(long, requires = "candidate_runner")]
        candidate_binary: Option<PathBuf>,
        /// Candidate runner source root containing the reason_runner Python package.
        #[arg(long, requires = "candidate_binary")]
        candidate_runner: Option<PathBuf>,
    },
}
#[path = "scenarios/agent_kernel.rs"]
mod case_0;
use case_0::*;
#[path = "scenarios/artifact_consumers.rs"]
mod case_1;
use case_1::*;
#[path = "scenarios/datasheet.rs"]
mod case_2;
use case_2::*;
#[path = "scenarios/frames.rs"]
mod case_3;
use case_3::*;
#[path = "scenarios/gateway/agent_gateway.rs"]
mod case_4;
use case_4::*;
#[path = "scenarios/installation.rs"]
mod case_5;
use case_5::*;
#[path = "scenarios/map.rs"]
mod case_6;
use case_6::*;
#[path = "scenarios/media.rs"]
mod case_7;
use case_7::*;
#[path = "scenarios/reason.rs"]
mod case_8;
use case_8::*;
#[path = "scenarios/recording_catalog_sdk.rs"]
mod case_9;
use case_9::*;
#[path = "scenarios/recording_fixture.rs"]
mod case_10;
use case_10::*;
#[path = "scenarios/recording_ingest.rs"]
mod case_11;
use case_11::*;
#[path = "scenarios/simulation.rs"]
mod case_12;
use case_12::*;
#[path = "scenarios/stream.rs"]
mod stream;
use stream::*;
#[path = "scenarios/sumo.rs"]
mod case_14;
use case_14::*;
#[path = "scenarios/view.rs"]
mod case_15;
use case_15::*;
fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
}
#[path = "scenarios/surreal.rs"]
mod surreal;
use surreal::surreal_integration;
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
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
    veoveo_testing_support::lifecycle::owner::run(execute()).await
}
async fn execute() -> Result<()> {
    install_rustls_provider();
    let args = Args::parse();
    match args.cmd {
        Cmd::SurrealIntegration => surreal_integration().await,
        Cmd::InstallationVerify {
            conformance_bin,
            installation,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

            let target = support::InstalledTarget::load(&installation)?;
            installation_verify(&conformance_bin, &target).await
        }
        Cmd::ArtifactUploadConsumers {
            conformance_bin,
            installation,
            browser_evidence,
            evidence_output,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

            let target = support::InstalledTarget::load(&installation)?;
            artifact_upload_consumers(
                &conformance_bin,
                &target,
                &browser_evidence,
                &evidence_output,
            )
            .await
        }
        Cmd::MediaMcpAuth {
            conformance_bin,
            media_bin,
            artifact_service_bin,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let media_bin = veoveo_testing_support::artifacts::requested_executable(
                media_bin,
                "veoveo-media-mcp",
                "media-mcp",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;
            media_mcp_auth(&conformance_bin, &media_bin, &artifact_service_bin).await
        }
        Cmd::MediaTaskRun {
            conformance_bin,
            media_bin,
            artifact_service_bin,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let media_bin = veoveo_testing_support::artifacts::requested_executable(
                media_bin,
                "veoveo-media-mcp",
                "media-mcp",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;
            media_task_run(&conformance_bin, &media_bin, &artifact_service_bin).await
        }
        Cmd::FramesMcp {
            conformance_bin,
            frames_bin,
            artifact_service_bin,
            installation,
            evidence_output,
        } => {
            if let Some(installation) = installation {
                return frames_installed(
                    &support::InstalledTarget::load(&installation)?,
                    &evidence_output.context("installed Frames requires --evidence-output")?,
                )
                .await;
            }
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;
            let frames_bin = veoveo_testing_support::artifacts::requested_executable(
                frames_bin,
                "veoveo-frames-mcp",
                "frames-mcp",
            )?;
            frames_mcp(&conformance_bin, &frames_bin, &artifact_service_bin).await
        }
        Cmd::MapMcp {
            conformance_bin,
            artifact_service_bin,
            map_image,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;
            map_mcp(&conformance_bin, &artifact_service_bin, &map_image).await
        }
        Cmd::ViewMcp {
            view_image,
            retained_frame,
            installation,
            installed_fixture,
            evidence_output,
        } => {
            if let Some(installation) = installation {
                view_installed(
                    &installation,
                    &installed_fixture.context("installed View requires --installed-fixture")?,
                    &evidence_output.context("installed View requires --evidence-output")?,
                )
                .await
            } else {
                view_mcp(&view_image, retained_frame.as_deref()).await
            }
        }
        Cmd::ViewFixtureExport { output } => {
            export_view_fixture(&output)?;
            println!(
                "{}",
                serde_json::json!({"schemaVersion":"veoveo.ai/view-fixture-preparation/v1","output":output,"qualification":false})
            );
            Ok(())
        }
        Cmd::ViewGoogleLive { view_image, output } => view_google_live(&view_image, &output).await,
        Cmd::DatasheetMcp {
            conformance_bin,
            artifact_service_bin,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;
            datasheet_mcp(&conformance_bin, &artifact_service_bin).await
        }
        Cmd::RecordingIngest {
            conformance_bin,
            gateway_bin,
            hub_bin,
            control_plane,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let hub_bin = veoveo_testing_support::artifacts::requested_executable(
                hub_bin,
                "veoveo-recording-hub",
                "spooler",
            )?;
            recording_ingest(&conformance_bin, &gateway_bin, &hub_bin, &control_plane).await
        }
        Cmd::AgentKernel {
            conformance_bin,
            media_bin,
            gateway_bin,
            control_plane,
            artifact_service_bin,
            agent_bin,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let media_bin = veoveo_testing_support::artifacts::requested_executable(
                media_bin,
                "veoveo-media-mcp",
                "media-mcp",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let agent_bin = veoveo_testing_support::artifacts::requested_executable(
                agent_bin,
                "veoveo-agent-kernel",
                "agent",
            )?;

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
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let media_bin = veoveo_testing_support::artifacts::requested_executable(
                media_bin,
                "veoveo-media-mcp",
                "media-mcp",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let agent_bin = veoveo_testing_support::artifacts::requested_executable(
                agent_bin,
                "veoveo-agent-kernel",
                "agent",
            )?;

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
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;
            let frames_bin = veoveo_testing_support::artifacts::requested_executable(
                frames_bin,
                "veoveo-frames-mcp",
                "frames-mcp",
            )?;
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let agent_bin = veoveo_testing_support::artifacts::requested_executable(
                agent_bin,
                "veoveo-agent-kernel",
                "agent",
            )?;
            let optimization_bin = veoveo_testing_support::artifacts::requested_executable(
                optimization_bin,
                "veoveo-optimization-mcp",
                "optimization-mcp",
            )?;

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
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let media_bin = veoveo_testing_support::artifacts::requested_executable(
                media_bin,
                "veoveo-media-mcp",
                "media-mcp",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let agent_bin = veoveo_testing_support::artifacts::requested_executable(
                agent_bin,
                "veoveo-agent-kernel",
                "agent",
            )?;

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
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            let artifact_service_bin = veoveo_testing_support::artifacts::requested_executable(
                artifact_service_bin,
                "veoveo-artifact-service",
                "artifact-service",
            )?;
            let gateway_bin = veoveo_testing_support::artifacts::requested_executable(
                gateway_bin,
                "veoveo-gateway-composition",
                "gateway",
            )?;
            let duckdb_bin = veoveo_testing_support::artifacts::requested_executable(
                duckdb_bin,
                "veoveo-duckdb-mcp",
                "duckdb-mcp",
            )?;

            agent_gateway(
                &conformance_bin,
                &duckdb_bin,
                &gateway_bin,
                &control_plane,
                &artifact_service_bin,
            )
            .await
        }
        Cmd::SumoPush { steps } => sumo_push(steps).await,
        Cmd::SumoVerify {
            conformance_bin,
            context,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;
            sumo_verify(&conformance_bin, &context).await
        }
        Cmd::SimulationCertify {
            deployment_lock,
            base_image,
            overlay_image,
            overlay_kind,
            source_revision,
            output,
            cache_directory,
            timeout_seconds,
        } => {
            simulation_certify(
                deployment_lock.as_deref(),
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
        Cmd::StreamCompilerStartup {
            installation,
            candidate_binary,
            candidate_app,
            work_dir,
        } => {
            stream_compiler_startup(
                &support::InstalledTarget::load(&installation)?,
                &candidate_binary,
                &candidate_app,
                &work_dir,
            )
            .await
        }
        Cmd::StreamGpu {
            installation,
            env_file,
            producer_key_secret,
            work_dir,
            candidate_binary,
            candidate_app,
            pipeline_id,
            replica_pods,
        } => {
            stream_gpu(
                &support::InstalledTarget::load(&installation)?,
                &env_file,
                &work_dir,
                candidate_binary.as_deref().zip(candidate_app.as_deref()),
                &pipeline_id,
                &producer_key_secret,
                &replica_pods,
            )
            .await
        }
        Cmd::RecordingFixtureFinish {
            installation,
            env_file,
            producer_key_secret,
            stream_ids,
        } => {
            recording_fixture_finish(
                &support::InstalledTarget::load(&installation)?,
                &env_file,
                &producer_key_secret,
                &stream_ids,
            )
            .await
        }
        Cmd::RecordingCatalogSdk {
            conformance_bin,
            installation,
            dataset_id,
            recording_id,
        } => {
            veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

            recording_catalog_sdk(
                &support::InstalledTarget::load(&installation)?,
                dataset_id,
                recording_id,
            )
            .await
        }
        Cmd::ReasonGpu {
            installation,
            env_file,
            work_dir,
            producer_key_secret,
            candidate_binary,
            candidate_runner,
        } => {
            reason_gpu(
                &support::InstalledTarget::load(&installation)?,
                &env_file,
                &work_dir,
                &producer_key_secret,
                candidate_binary.as_deref().zip(candidate_runner.as_deref()),
            )
            .await
        }
    }
}

#[cfg(test)]
mod view_cli_tests {
    use super::*;
    use anyhow::ensure;
    #[test]
    fn installed_view_requires_complete_explicit_fixture_flags() {
        assert!(Args::try_parse_from(["installation-smoke", "view-mcp"]).is_ok());
        for args in [
            vec![
                "installation-smoke",
                "view-mcp",
                "--installation",
                "target.json",
            ],
            vec![
                "installation-smoke",
                "view-mcp",
                "--installed-fixture",
                "fixture.json",
            ],
            vec![
                "installation-smoke",
                "view-mcp",
                "--evidence-output",
                "receipt.json",
            ],
        ] {
            assert!(Args::try_parse_from(args).is_err());
        }
        assert!(
            Args::try_parse_from([
                "installation-smoke",
                "view-mcp",
                "--installation",
                "target.json",
                "--installed-fixture",
                "fixture.json",
                "--evidence-output",
                "receipt.json"
            ])
            .is_ok()
        );
    }
    #[test]
    fn fixture_export_is_separate_from_registered_view_qualification() -> Result<()> {
        let args = Args::try_parse_from([
            "installation-smoke",
            "view-fixture-export",
            "--output",
            "fixture",
        ])?;
        ensure!(matches!(args.cmd, Cmd::ViewFixtureExport { .. }));
        let descriptors: Value = serde_json::from_str(include_str!("../../smoke/scenarios.json"))?;
        // A preparation utility cannot be selected as an acceptance scenario.
        ensure!(!descriptors.to_string().contains("view-fixture-export"));
        Ok(())
    }
}

#[cfg(test)]
mod frames_cli_tests {
    use super::*;
    #[test]
    fn installed_frames_requires_paired_explicit_inputs_and_preserves_local_default() {
        assert!(Args::try_parse_from(["installation-smoke", "frames-mcp"]).is_ok());
        for flag in ["--installation", "--evidence-output"] {
            assert!(
                Args::try_parse_from(["installation-smoke", "frames-mcp", flag, "path.json"])
                    .is_err()
            );
        }
        assert!(
            Args::try_parse_from([
                "installation-smoke",
                "frames-mcp",
                "--installation",
                "target.json",
                "--evidence-output",
                "receipt.json"
            ])
            .is_ok()
        );
    }
}
