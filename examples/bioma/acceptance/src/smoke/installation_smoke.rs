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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum, veoveo_types::Vocabulary)]
enum InstallationScope {
    #[default]
    Full,
    Duckdb,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
enum ArtifactProfileChoice {
    #[default]
    Browser,
    Focused,
}
#[derive(Subcommand, Debug)]
enum Cmd {
    SurrealIntegration,
    InstallationVerify {
        /// Select the complete installation gate or the CPU DuckDB owner gate.
        #[arg(long, value_enum, default_value_t = InstallationScope::Full)]
        scope: InstallationScope,
        /// Required new private receipt for the focused DuckDB scope.
        #[arg(long, required_if_eq("scope", "duckdb"))]
        evidence_output: Option<PathBuf>,
        /// Built conformance binary used as the public machine OAuth and MCP client.
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        /// Installation-owned target and public control-plane coordinates.
        #[arg(long)]
        installation: PathBuf,
    },
    InstalledProtocol {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long)]
        installed_fixture: PathBuf,
        #[arg(long)]
        evidence_output: PathBuf,
    },
    InstalledHost {
        /// Installation-owned public OAuth target and control plane.
        #[arg(long)]
        installation: PathBuf,
        /// Existing database owned by the selected operator.
        #[arg(long)]
        database: veoveo_duckdb_mcp::DuckDbDatabaseId,
        /// Private declaration of the existing DuckDB deployment, Pod and container.
        #[arg(long)]
        installed_fixture: PathBuf,
        /// New private receipt file; existing files refuse execution.
        #[arg(long)]
        evidence_output: PathBuf,
    },
    ArtifactUploadConsumers {
        #[arg(long)]
        conformance_bin: Option<PathBuf>,
        #[arg(long)]
        installation: PathBuf,
        /// Select browser large-file evidence or focused bounded machine uploads.
        #[arg(long, value_enum)]
        profile: Option<ArtifactProfileChoice>,
        #[arg(
            long,
            required_unless_present = "profile",
            required_if_eq("profile", "browser")
        )]
        browser_evidence: Option<PathBuf>,
        #[arg(long)]
        evidence_output: PathBuf,
        /// Opt in to the selected Artifact service recovery fixture.
        #[arg(long)]
        service_recovery: bool,
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
    },
    FramesRecovery {
        #[arg(long)]
        installation: PathBuf,
        #[arg(long)]
        crash_fixture: PathBuf,
        #[arg(long)]
        marker_output: PathBuf,
        #[arg(long)]
        evidence_output: PathBuf,
    },
    FramesInstalled {
        /// Installation-owned normal OAuth target and public control plane.
        #[arg(long)]
        installation: PathBuf,
        /// New private receipt file; existing files refuse execution.
        #[arg(long)]
        evidence_output: PathBuf,
    },
    FramesReferenceInstalled {
        /// Installation-owned normal operator OAuth target and public control plane.
        #[arg(long)]
        installation: PathBuf,
        /// New private receipt for the retained reference world and revision.
        #[arg(long)]
        evidence_output: PathBuf,
    },
    TimeseriesInstalled {
        /// Installation-owned normal OAuth target and public control plane.
        #[arg(long)]
        installation: PathBuf,
        /// New private receipt for the retained forecast Task and Artifact.
        #[arg(long)]
        evidence_output: PathBuf,
        /// Explicit cancellation or reconnect workload fixture.
        #[arg(long)]
        lifecycle_input: Option<PathBuf>,
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
#[path = "scenarios/timeseries.rs"]
mod timeseries;
use timeseries::timeseries_installed;
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
            scope,
            evidence_output,
        } => {
            if matches!(scope, InstallationScope::Duckdb) {
                let evidence =
                    evidence_output.context("DuckDB scope requires --evidence-output")?;
                let target = support::InstalledTarget::load(&installation)?;
                let control_plane = target.target.control_plane_path(&installation);
                return case_5::installed_duckdb::run(&target, &control_plane, &evidence).await;
            }
            anyhow::ensure!(
                evidence_output.is_none(),
                "--evidence-output requires --scope duckdb"
            );
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

            let target = support::InstalledTarget::load(&installation)?;
            installation_verify(&conformance_bin, &target).await
        }
        Cmd::InstalledProtocol {
            installation,
            installed_fixture,
            evidence_output,
        } => case_5::protocol::run(&installation, &installed_fixture, &evidence_output).await,
        Cmd::InstalledHost {
            installation,
            database,
            installed_fixture,
            evidence_output,
        } => {
            case_5::shared_host::run(
                &support::InstalledTarget::load(&installation)?,
                &database,
                &installed_fixture,
                &evidence_output,
            )
            .await
        }
        Cmd::ArtifactUploadConsumers {
            conformance_bin,
            installation,
            browser_evidence,
            profile,
            evidence_output,
            service_recovery,
        } => {
            let selected = ArtifactConsumerProfile::admit(
                profile.unwrap_or_default() == ArtifactProfileChoice::Focused,
                browser_evidence,
            )?;
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

            let target = support::InstalledTarget::load(&installation)?;
            artifact_upload_consumers(
                &conformance_bin,
                &target,
                selected,
                &evidence_output,
                service_recovery,
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
        Cmd::FramesRecovery {
            installation,
            crash_fixture,
            marker_output,
            evidence_output,
        } => {
            frames_recovery(
                &installation,
                &crash_fixture,
                &marker_output,
                &evidence_output,
            )
            .await
        }
        Cmd::FramesInstalled {
            installation,
            evidence_output,
        } => {
            frames_installed(
                &support::InstalledTarget::load(&installation)?,
                &evidence_output,
            )
            .await
        }
        Cmd::FramesReferenceInstalled {
            installation,
            evidence_output,
        } => {
            frames_reference_installed(
                &support::InstalledTarget::load(&installation)?,
                &evidence_output,
            )
            .await
        }
        Cmd::TimeseriesInstalled {
            installation,
            evidence_output,
            lifecycle_input,
        } => {
            timeseries_installed(
                &support::InstalledTarget::load(&installation)?,
                &evidence_output,
                lifecycle_input.as_deref(),
            )
            .await
        }
        Cmd::FramesMcp {
            conformance_bin,
            frames_bin,
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
    fn installed_frames_requires_explicit_inputs_and_local_rejects_installed_flags() -> Result<()> {
        let local = Args::try_parse_from(["installation-smoke", "frames-mcp"])?;
        assert!(matches!(
            local.cmd,
            Cmd::FramesMcp {
                conformance_bin: None,
                frames_bin: None,
                artifact_service_bin: None
            }
        ));
        for args in [
            vec!["installation-smoke", "frames-installed"],
            vec![
                "installation-smoke",
                "frames-installed",
                "--installation",
                "target.json",
            ],
            vec![
                "installation-smoke",
                "frames-installed",
                "--evidence-output",
                "receipt.json",
            ],
            vec![
                "installation-smoke",
                "frames-mcp",
                "--installation",
                "target.json",
            ],
            vec![
                "installation-smoke",
                "frames-mcp",
                "--evidence-output",
                "receipt.json",
            ],
            vec![
                "installation-smoke",
                "frames-mcp",
                "--installation",
                "target.json",
                "--evidence-output",
                "receipt.json",
            ],
        ] {
            assert!(Args::try_parse_from(args).is_err());
        }
        let installed = Args::try_parse_from([
            "installation-smoke",
            "frames-installed",
            "--installation",
            "target.json",
            "--evidence-output",
            "receipt.json",
        ])?;
        let Cmd::FramesInstalled {
            installation,
            evidence_output,
        } = installed.cmd
        else {
            bail!("wrong installed Frames command");
        };
        assert_eq!(installation, PathBuf::from("target.json"));
        assert_eq!(evidence_output, PathBuf::from("receipt.json"));
        for incomplete in [
            vec!["installation-smoke", "timeseries-installed"],
            vec![
                "installation-smoke",
                "timeseries-installed",
                "--installation",
                "target.json",
            ],
            vec![
                "installation-smoke",
                "timeseries-installed",
                "--evidence-output",
                "receipt.json",
            ],
            vec![
                "installation-smoke",
                "timeseries-installed",
                "--installation",
                "target.json",
                "--evidence-output",
                "receipt.json",
                "--timeseries-bin",
                "local-server",
            ],
        ] {
            assert!(Args::try_parse_from(incomplete).is_err());
        }
        let installed = Args::try_parse_from([
            "installation-smoke",
            "timeseries-installed",
            "--installation",
            "target.json",
            "--evidence-output",
            "receipt.json",
        ])?;
        let Cmd::TimeseriesInstalled {
            installation,
            evidence_output,
            lifecycle_input,
        } = installed.cmd
        else {
            bail!("wrong installed Timeseries command");
        };
        assert_eq!(installation, PathBuf::from("target.json"));
        assert_eq!(evidence_output, PathBuf::from("receipt.json"));
        assert!(lifecycle_input.is_none());
        for incomplete in [
            vec!["installation-smoke", "frames-reference-installed"],
            vec![
                "installation-smoke",
                "frames-reference-installed",
                "--installation",
                "target.json",
            ],
            vec![
                "installation-smoke",
                "frames-reference-installed",
                "--evidence-output",
                "receipt.json",
            ],
            vec![
                "installation-smoke",
                "frames-reference-installed",
                "--installation",
                "target.json",
                "--evidence-output",
                "receipt.json",
                "--frames-bin",
                "local-server",
            ],
        ] {
            assert!(Args::try_parse_from(incomplete).is_err());
        }
        let references = Args::try_parse_from([
            "installation-smoke",
            "frames-reference-installed",
            "--installation",
            "target.json",
            "--evidence-output",
            "receipt.json",
        ])?;
        let Cmd::FramesReferenceInstalled {
            installation,
            evidence_output,
        } = references.cmd
        else {
            bail!("wrong installed Frames reference command");
        };
        assert_eq!(installation, PathBuf::from("target.json"));
        assert_eq!(evidence_output, PathBuf::from("receipt.json"));
        let local = Args::try_parse_from([
            "installation-smoke",
            "frames-mcp",
            "--conformance-bin",
            "conformance",
            "--frames-bin",
            "frames",
            "--artifact-service-bin",
            "artifacts",
        ])?;
        let Cmd::FramesMcp {
            conformance_bin,
            frames_bin,
            artifact_service_bin,
        } = local.cmd
        else {
            bail!("wrong local Frames command");
        };
        assert_eq!(conformance_bin, Some(PathBuf::from("conformance")));
        assert_eq!(frames_bin, Some(PathBuf::from("frames")));
        assert_eq!(artifact_service_bin, Some(PathBuf::from("artifacts")));
        Ok(())
    }
    #[test]
    fn lifecycle_profiles_require_explicit_cli_selection() -> Result<()> {
        let focused = Args::try_parse_from([
            "installation-smoke",
            "artifact-upload-consumers",
            "--installation",
            "target.json",
            "--profile",
            "focused",
            "--evidence-output",
            "receipt.json",
        ])?;
        let Cmd::ArtifactUploadConsumers {
            profile,
            browser_evidence,
            ..
        } = focused.cmd
        else {
            bail!("wrong Artifact command")
        };
        assert!(matches!(
            ArtifactConsumerProfile::admit(
                profile.unwrap_or_default() == ArtifactProfileChoice::Focused,
                browser_evidence
            )?,
            ArtifactConsumerProfile::Focused
        ));
        assert!(ArtifactConsumerProfile::admit(true, Some(PathBuf::from("browser.json"))).is_err());
        assert!(ArtifactConsumerProfile::admit(false, None).is_err());
        assert!(
            Args::try_parse_from([
                "installation-smoke",
                "artifact-upload-consumers",
                "--installation",
                "target.json",
                "--evidence-output",
                "receipt.json"
            ])
            .is_err()
        );
        assert!(
            Args::try_parse_from([
                "installation-smoke",
                "artifact-upload-consumers",
                "--installation",
                "target.json",
                "--profile",
                "browser",
                "--evidence-output",
                "receipt.json"
            ])
            .is_err()
        );
        for selected in [false, true] {
            let mut args = vec![
                "installation-smoke",
                "artifact-upload-consumers",
                "--installation",
                "target.json",
                "--browser-evidence",
                "browser.json",
                "--evidence-output",
                "receipt.json",
            ];
            if selected {
                args.push("--service-recovery");
            }
            let Cmd::ArtifactUploadConsumers {
                service_recovery, ..
            } = Args::try_parse_from(args)?.cmd
            else {
                bail!("wrong Artifact command")
            };
            assert_eq!(service_recovery, selected);
            let mut args = vec![
                "installation-smoke",
                "timeseries-installed",
                "--installation",
                "target.json",
                "--evidence-output",
                "receipt.json",
            ];
            if selected {
                args.extend(["--lifecycle-input", "lifecycle.json"]);
            }
            let Cmd::TimeseriesInstalled {
                lifecycle_input, ..
            } = Args::try_parse_from(args)?.cmd
            else {
                bail!("wrong Timeseries command")
            };
            assert_eq!(
                lifecycle_input,
                selected.then(|| PathBuf::from("lifecycle.json"))
            );
        }
        Ok(())
    }

    #[test]
    fn frames_descriptors_keep_installed_preparation_separate_from_local_coverage() -> Result<()> {
        use veoveo_testing_support::descriptor::{
            BuildProfile, PackageName, Preparation, ScenarioDescriptor,
        };
        let descriptor: ScenarioDescriptor =
            serde_json::from_str(include_str!("../../smoke/scenarios.json"))?;
        for id in [
            "frames-installed",
            "frames-reference-installed",
            "timeseries-installed",
        ] {
            let installed = descriptor
                .scenarios
                .iter()
                .find(|scenario| scenario.id.as_str() == id)
                .context("installed domain scenario is absent")?;
            assert_eq!(installed.arguments, [id]);
            let veoveo_testing_support::descriptor::HarnessTarget::CargoBinary {
                selection: harness,
            } = &installed.target
            else {
                bail!("installed domain scenario requires its existing assertion executable");
            };
            assert_eq!(harness.owner, PathBuf::from("examples/bioma/acceptance"));
            assert_eq!(
                harness.package,
                PackageName::parse("veoveo-bioma-acceptance")?
            );
            assert_eq!(harness.target, "installation-smoke");
            assert_eq!(harness.features, ["smoke".to_string()].into());
            assert!(!harness.default_features);
            assert_eq!(harness.profile, BuildProfile::Dev);
            assert_eq!(installed.deadline_seconds, 3600);
            assert_eq!(installed.cleanup_seconds, 180);
            let [Preparation::CargoBinary { selection }] = installed.prerequisites.as_slice()
            else {
                bail!("installed domain scenario requires exactly its OAuth utility");
            };
            assert_eq!(
                selection.owner,
                PathBuf::from("platform/gateway/composition")
            );
            assert_eq!(
                selection.package,
                PackageName::parse("veoveo-gateway-composition")?
            );
            assert_eq!(selection.target, "gateway-smoke-support");
            assert_eq!(selection.features, ["smoke".to_string()].into());
            assert!(!selection.default_features);
            assert_eq!(selection.profile, BuildProfile::Dev);
            assert!(installed.requirements.network && installed.requirements.credentials);
            assert!(
                !installed.requirements.nvidia
                    && !installed.requirements.headed_graphics
                    && !installed.requirements.cluster_mutation
                    && !installed.requirements.billed_effects
            );
        }
        let local = descriptor
            .scenarios
            .iter()
            .find(|scenario| scenario.id.as_str() == "frames-mcp")
            .context("local Frames scenario is absent")?;
        assert_eq!(local.arguments, ["frames-mcp"]);
        assert_eq!(local.prerequisites.len(), 8);
        let local_targets = local
            .prerequisites
            .iter()
            .map(|preparation| match preparation {
                Preparation::CargoBinary { selection } => Ok(selection.target.as_str()),
                _ => bail!("local Frames requires its native executables"),
            })
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        assert_eq!(
            local_targets,
            [
                "conformance",
                "gateway-smoke-support",
                "media-smoke",
                "agent-smoke",
                "artifact-smoke",
                "deployment-fixtures",
                "frames-mcp",
                "artifact-service"
            ]
            .into()
        );
        Ok(())
    }
}

#[cfg(test)]
mod installed_host_cli_tests {
    use super::*;
    #[test]
    fn installed_host_requires_complete_explicit_inputs_and_typed_database() -> Result<()> {
        let flags = [
            ("--installation", "target.json"),
            ("--database", "owned_fixture"),
            ("--installed-fixture", "/private/fixture.json"),
            ("--evidence-output", "/private/new-receipt.json"),
        ];
        for missing in 0..flags.len() {
            let mut args = vec!["installation-smoke", "installed-host"];
            for (index, (name, value)) in flags.iter().enumerate() {
                if index != missing {
                    args.extend([*name, *value]);
                }
            }
            assert!(Args::try_parse_from(args).is_err());
        }
        let mut args = vec!["installation-smoke", "installed-host"];
        for (name, value) in flags {
            args.extend([name, value]);
        }
        let parsed = Args::try_parse_from(args.clone())?;
        let Cmd::InstalledHost { database, .. } = parsed.cmd else {
            bail!("wrong installed Host command");
        };
        assert_eq!(
            database,
            veoveo_duckdb_mcp::DuckDbDatabaseId::new("owned_fixture")?
        );
        let mut malformed = args.clone();
        malformed[5] = "../foreign";
        assert!(Args::try_parse_from(malformed).is_err());
        for unsupported in ["--fixture", "--pod", "--context"] {
            let mut undeclared = args.clone();
            undeclared.extend([unsupported, "foreign"]);
            assert!(Args::try_parse_from(undeclared).is_err());
        }
        Ok(())
    }
    #[test]
    fn installed_host_descriptor_declares_cpu_process_mutation() -> Result<()> {
        let descriptor: veoveo_testing_support::descriptor::ScenarioDescriptor =
            serde_json::from_str(include_str!("../../smoke/scenarios.json"))?;
        let scenario = descriptor
            .scenarios
            .iter()
            .find(|scenario| scenario.id.as_str() == "installed-host")
            .context("installed Host scenario is absent")?;
        assert_eq!(scenario.arguments, ["installed-host"]);
        assert!(
            scenario.requirements.network
                && scenario.requirements.credentials
                && scenario.requirements.cluster_mutation
        );
        assert!(
            !scenario.requirements.nvidia
                && !scenario.requirements.headed_graphics
                && !scenario.requirements.billed_effects
        );
        Ok(())
    }
}

#[cfg(test)]
mod installed_protocol_cli_tests {
    use super::*;
    #[test]
    fn protocol_requires_all_three_explicit_inputs_and_refuses_aliases() {
        for arguments in [
            vec!["installation-smoke", "installed-protocol"],
            vec![
                "installation-smoke",
                "installed-protocol",
                "--installation",
                "target.json",
            ],
            vec![
                "installation-smoke",
                "installed-protocol",
                "--installation",
                "target.json",
                "--installed-fixture",
                "fixture.json",
            ],
            vec![
                "installation-smoke",
                "installed-protocol",
                "--installation",
                "target.json",
                "--evidence-output",
                "receipt.json",
            ],
        ] {
            assert!(Args::try_parse_from(arguments).is_err());
        }
        let parsed = Args::try_parse_from([
            "installation-smoke",
            "installed-protocol",
            "--installation",
            "target.json",
            "--installed-fixture",
            "fixture.json",
            "--evidence-output",
            "receipt.json",
        ])
        .unwrap();
        assert!(matches!(parsed.cmd, Cmd::InstalledProtocol { .. }));
        assert!(
            Args::try_parse_from([
                "installation-smoke",
                "installed-protocol",
                "--installation",
                "target.json",
                "--fixture",
                "fixture.json",
                "--evidence-output",
                "receipt.json"
            ])
            .is_err()
        );
    }
    #[test]
    fn protocol_descriptor_selects_only_owner_and_oauth_helper() {
        let descriptors: veoveo_testing_support::descriptor::ScenarioDescriptor =
            serde_json::from_str(include_str!("../../smoke/scenarios.json")).unwrap();
        let selected = descriptors
            .scenarios
            .iter()
            .find(|scenario| scenario.id.as_str() == "installed-protocol")
            .unwrap();
        assert_eq!(selected.prerequisites.len(), 1);
        assert!(!selected.requirements.cluster_mutation);
        assert!(!selected.requirements.nvidia);
        assert!(!selected.requirements.billed_effects);
    }
}

#[cfg(test)]
mod frames_recovery_cli_tests {
    use super::*;
    #[test]
    fn recovery_requires_all_four_explicit_inputs_and_refuses_legacy_aliases() {
        let flags = [
            "--installation",
            "--crash-fixture",
            "--marker-output",
            "--evidence-output",
        ];
        for omitted in 0..flags.len() {
            let mut args = vec!["installation-smoke", "frames-recovery"];
            for (index, flag) in flags.iter().enumerate() {
                if index != omitted {
                    args.extend([*flag, "fixture-path"]);
                }
            }
            assert!(Args::try_parse_from(args).is_err());
        }
        let parsed = Args::try_parse_from([
            "installation-smoke",
            "frames-recovery",
            "--installation",
            "target.json",
            "--crash-fixture",
            "crash.json",
            "--marker-output",
            "ready.json",
            "--evidence-output",
            "result.json",
        ])
        .unwrap();
        assert!(matches!(parsed.cmd, Cmd::FramesRecovery { .. }));
        assert!(
            Args::try_parse_from([
                "installation-smoke",
                "frames-recovery",
                "--installation",
                "target.json",
                "--fixture",
                "crash.json",
                "--marker-output",
                "ready.json",
                "--evidence-output",
                "result.json"
            ])
            .is_err()
        );
    }
    #[test]
    fn recovery_descriptor_selects_only_existing_assertion_and_oauth_helper() {
        use veoveo_testing_support::descriptor::{HarnessTarget, Preparation, ScenarioDescriptor};
        let descriptors: ScenarioDescriptor =
            serde_json::from_str(include_str!("../../smoke/scenarios.json")).unwrap();
        let selected = descriptors
            .scenarios
            .iter()
            .find(|scenario| scenario.id.as_str() == "frames-recovery")
            .unwrap();
        let HarnessTarget::CargoBinary { selection } = &selected.target else {
            panic!("expected maintained assertion binary");
        };
        assert_eq!(selection.package.as_str(), "veoveo-bioma-acceptance");
        assert_eq!(selection.target, "installation-smoke");
        assert!(!selection.default_features);
        assert_eq!(selected.prerequisites.len(), 1);
        let Preparation::CargoBinary { selection } = &selected.prerequisites[0] else {
            panic!("expected OAuth helper");
        };
        assert_eq!(selection.package.as_str(), "veoveo-gateway-composition");
        assert_eq!(selection.target, "gateway-smoke-support");
        assert!(!selection.default_features);
        assert!(selected.requirements.cluster_mutation);
        assert!(
            !selected.requirements.nvidia
                && !selected.requirements.billed_effects
                && !selected.requirements.headed_graphics
        );
    }
}

#[cfg(test)]
mod duckdb_scope_cli_tests {
    use super::*;
    #[test]
    fn focused_scope_requires_receipt_and_preserves_full_default() -> Result<()> {
        assert!(
            Args::try_parse_from([
                "installation-smoke",
                "installation-verify",
                "--installation",
                "target.json",
                "--scope",
                "duckdb"
            ])
            .is_err()
        );
        let focused = Args::try_parse_from([
            "installation-smoke",
            "installation-verify",
            "--installation",
            "target.json",
            "--scope",
            "duckdb",
            "--evidence-output",
            "private.json",
        ])?;
        assert!(matches!(
            focused.cmd,
            Cmd::InstallationVerify {
                scope: InstallationScope::Duckdb,
                evidence_output: Some(_),
                ..
            }
        ));
        let full = Args::try_parse_from([
            "installation-smoke",
            "installation-verify",
            "--installation",
            "target.json",
        ])?;
        assert!(matches!(
            full.cmd,
            Cmd::InstallationVerify {
                scope: InstallationScope::Full,
                evidence_output: None,
                ..
            }
        ));
        assert!(
            Args::try_parse_from([
                "installation-smoke",
                "installation-verify",
                "--installation",
                "target.json",
                "--scope",
                "unknown"
            ])
            .is_err()
        );
        Ok(())
    }
}
