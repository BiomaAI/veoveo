use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veoveo_recording_contract::RecordingId;

#[allow(dead_code)]
mod browser;
mod cli;
mod support;
use support::InstalledTarget;
mod restart;
mod source_timeline;

use source_timeline::{
    SOURCE_SAMPLE_TIMEOUT, SourceTimelineAlignmentEvidence, SourceTimelineWindowEvidence,
    sample_source_alignment, source_timeline_sample, source_timeline_window,
};

use cli::{Args, SmokeCommand};

use browser::{
    ConsoleAgentInstructionEvidence, ConsoleAppExpectation, ConsoleAppSettledState,
    ConsoleAppsCatalogEvidence, ConsoleLiveCaptureEvidence, ConsoleLiveGridEvidence,
    ConsoleMapWorkspaceCaptureEvidence, ConsoleRecordingArchiveCaptureEvidence,
    ConsoleRecordingCaptureEvidence, MapWorkspaceCaptureEvidence, capture_console_apps_catalog,
    capture_console_live_app, capture_console_live_app_five_user_grid,
    capture_console_live_app_grid, capture_console_live_app_pair,
    capture_console_map_workspace_app, capture_console_recording,
    capture_console_recording_archive, capture_map_workspace_app, preflight_console_live_app,
    preflight_standalone_live_app, send_console_uav_agent_instruction,
};
use restart::{RestartVerification, verify_live_view_restarts};

const EVIDENCE_SCHEMA: &str = "veoveo.ai/uav-live-view-browser-evidence/v12";
const MAX_RECORDING_SOURCE_LAG_SECONDS: f64 = 1.0;
const MINIMUM_PHYSICS_REAL_TIME_FACTOR: f64 = 0.98;
const PRIMARY_CAMERA_ID: &str = "follow";
const QUALIFIED_CAMERA_IDS: [&str; 5] = [
    PRIMARY_CAMERA_ID,
    "chase",
    "orbit",
    "stabilized",
    "formation",
];
const FOCUSED_UAV_APP_HOST_PREFLIGHTS: [FocusedUavAppHostPreflight; 2] = [
    FocusedUavAppHostPreflight::Console,
    FocusedUavAppHostPreflight::Standalone,
];
const FIRST_PARTY_CONSOLE_APPS: [ConsoleAppExpectation; 16] = [
    ConsoleAppExpectation {
        server: "artifact",
        resource_uri: "ui://artifact/library.html",
        marker: "Library",
        settled_state: ConsoleAppSettledState::Exact(&["ready"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "recording",
        resource_uri: "ui://recording/explorer.html",
        marker: "Explorer",
        settled_state: ConsoleAppSettledState::Exact(&["ready"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "optimization",
        resource_uri: "ui://optimization/routes.html",
        marker: "Routes",
        settled_state: ConsoleAppSettledState::Exact(&["ready"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "optimization",
        resource_uri: "ui://optimization/models.html",
        marker: "Models",
        settled_state: ConsoleAppSettledState::Exact(&["ready"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "reason",
        resource_uri: "ui://reason/analyses.html",
        marker: "Analyses",
        settled_state: ConsoleAppSettledState::Exact(&["ready"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "media",
        resource_uri: "ui://media/studio.html",
        marker: "Studio",
        settled_state: ConsoleAppSettledState::Exact(&["ready"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "duckdb",
        resource_uri: "ui://duckdb/workbench.html",
        marker: "Workbench",
        settled_state: ConsoleAppSettledState::Exact(&["ready"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "datasheet",
        resource_uri: "ui://datasheet/workbench.html",
        marker: "Workbench",
        settled_state: ConsoleAppSettledState::Exact(&["ready"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "frames",
        resource_uri: "ui://frames/workspace.html",
        marker: "Frame Editor",
        settled_state: ConsoleAppSettledState::Exact(&["ready"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "time",
        resource_uri: "ui://time/timeline.html",
        marker: "Timeline",
        settled_state: ConsoleAppSettledState::Exact(&["ready"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "charts",
        resource_uri: "ui://charts/composer.html",
        marker: "Composer",
        settled_state: ConsoleAppSettledState::Exact(&["rendered"]),
        required_selector: Some("#canvas img"),
    },
    ConsoleAppExpectation {
        server: "map",
        resource_uri: "ui://map/workspace.html",
        marker: "Map Explorer",
        settled_state: ConsoleAppSettledState::MapViewport,
        required_selector: Some(".maplibregl-canvas"),
    },
    ConsoleAppExpectation {
        server: "stream",
        resource_uri: "ui://stream/live.html",
        marker: "Live Monitor",
        settled_state: ConsoleAppSettledState::Exact(&["waiting", "running", "stopped"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "timeseries",
        resource_uri: "ui://timeseries/forecast.html",
        marker: "Forecasts",
        settled_state: ConsoleAppSettledState::Exact(&["Waiting for forecast data…"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "uav-sim",
        resource_uri: "ui://uav-sim/live.html",
        marker: "Live Cameras",
        settled_state: ConsoleAppSettledState::Exact(&["ready", "live"]),
        required_selector: None,
    },
    ConsoleAppExpectation {
        server: "view",
        resource_uri: "ui://view/preview.html",
        marker: "Preview",
        settled_state: ConsoleAppSettledState::Prefix(&["ready —", "composition "]),
        required_selector: Some("#gl"),
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FocusedUavAppHostPreflight {
    Console,
    Standalone,
}

impl FocusedUavAppHostPreflight {
    const fn label(self) -> &'static str {
        match self {
            Self::Console => "console",
            Self::Standalone => "standalone",
        }
    }
}

#[cfg(test)]
fn focused_uav_app_host_preflights() -> [&'static str; 2] {
    FOCUSED_UAV_APP_HOST_PREFLIGHTS.map(FocusedUavAppHostPreflight::label)
}

#[derive(Debug, Deserialize)]
struct FocusedScenario {
    session_id: String,
    vehicle_id: String,
    view: FocusedView,
}

#[derive(Debug, Deserialize)]
struct FocusedView {
    timeout_seconds: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserAcceptanceEvidence {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    source_revision: String,
    run_id: String,
    scenario_path: String,
    session_id: String,
    camera_ids: Vec<String>,
    source_window: SourceTimelineWindowEvidence,
    sensor_isolation: SensorIsolationEvidence,
    performance: LiveViewPerformanceEvidence,
    grid: ConsoleLiveGridEvidence,
    concurrent_users: Vec<ConsoleLiveGridEvidence>,
    live_views: Vec<ConsoleLiveCaptureEvidence>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingBrowserAcceptanceEvidence {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    source_revision: String,
    run_id: String,
    scenario_path: String,
    session_id: String,
    recording_id: RecordingId,
    source_simulation_time_seconds: f64,
    recording_simulation_time_seconds: f64,
    recording_source_lag_seconds: f64,
    source_alignment: SourceTimelineAlignmentEvidence,
    recording: ConsoleRecordingCaptureEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingArchiveBrowserAcceptanceEvidence {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    source_revision: String,
    run_id: String,
    recording_id: RecordingId,
    recording: ConsoleRecordingArchiveCaptureEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentInstructionBrowserAcceptanceEvidence {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    source_revision: String,
    run_id: String,
    instruction: ConsoleAgentInstructionEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MapWorkspaceBrowserAcceptanceEvidence {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    source_revision: String,
    run_id: String,
    workspace: MapWorkspaceCaptureEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MapWorkspaceLiveBrowserAcceptanceEvidence {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    source_revision: String,
    run_id: String,
    workspace: ConsoleMapWorkspaceCaptureEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConsoleAppsBrowserAcceptanceEvidence {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    source_revision: String,
    run_id: String,
    catalog: ConsoleAppsCatalogEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SensorIsolationEvidence {
    vehicle_id: String,
    declared_frame_rate_hz: f64,
    frames_before: u64,
    frames_after: u64,
    simulation_seconds: f64,
    observed_frame_rate_hz: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveViewPerformanceEvidence {
    physics_real_time_factor: f64,
    qualified_camera_count: usize,
    browser_user_count: usize,
    simultaneous_camera_views: usize,
    simultaneous_encoded_streams: usize,
    minimum_observed_frame_rate_hz: f64,
    maximum_observed_frame_rate_hz: f64,
    browser_dropped_frames: u64,
    maximum_source_to_render_p95_ms: f64,
    maximum_composed_motion_to_photon_upper_bound_p95_ms: f64,
}

struct OperatorClient<'a> {
    conformance: &'a Path,
    installation: &'a InstalledTarget,
    token: &'a str,
}

impl OperatorClient<'_> {
    async fn conformance(&self, operation: &[&str], timeout: Duration) -> Result<String> {
        gateway_conformance(
            self.conformance,
            self.installation,
            self.token,
            operation,
            timeout,
        )
        .await
    }

    async fn call_tool(&self, tool: &str, arguments: Value) -> Result<Value> {
        let arguments = serde_json::to_string(&arguments)?;
        let output = self
            .conformance(
                &["call", "--tool-name", tool, "--arguments", &arguments],
                Duration::from_secs(30),
            )
            .await?;
        structured_output(&output).with_context(|| format!("tool {tool} returned invalid output"))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    veoveo_testing_support::lifecycle::owner::run(execute()).await
}
async fn execute() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    match Args::parse().command {
        SmokeCommand::WorkspaceMarkdownVerify {
            installation,
            chrome_cdp_url,
            evidence_root,
        } => {
            let installation = InstalledTarget::load(&installation)?;
            browser::artifact_upload::workspace::verify(
                installation.public_base(),
                &chrome_cdp_url,
                &evidence_root,
            )
            .await
        }
        SmokeCommand::SpeechWorkspaceVerify {
            installation,
            chrome_cdp_url,
            evidence_root,
            audio_fixture,
        } => {
            let installation = InstalledTarget::load(&installation)?;
            browser::speech::verify(
                installation.public_base(),
                &chrome_cdp_url,
                &evidence_root,
                &audio_fixture,
            )
            .await
        }
        SmokeCommand::MapWorkspaceBrowserVerify {
            chrome_cdp_url,
            app_html,
            evidence_root,
            timeout_seconds,
        } => {
            verify_map_workspace(
                &chrome_cdp_url,
                &app_html,
                &evidence_root,
                Duration::from_secs(timeout_seconds),
            )
            .await
        }
        SmokeCommand::MapWorkspaceLiveBrowserVerify {
            installation,
            composition_title,
            layer_title,
            chrome_cdp_url,
            evidence_root,
            timeout_seconds,
        } => {
            let installation = InstalledTarget::load(&installation)?;
            verify_live_map_workspace(
                installation.public_base(),
                &composition_title,
                &layer_title,
                &chrome_cdp_url,
                &evidence_root,
                Duration::from_secs(timeout_seconds),
            )
            .await
        }
        SmokeCommand::UavAppHostsBrowserVerify {
            installation,
            chrome_cdp_url,
            timeout_seconds,
        } => {
            let installation = InstalledTarget::load(&installation)?;
            verify_uav_app_hosts(
                installation.public_base(),
                &chrome_cdp_url,
                Duration::from_secs(timeout_seconds),
            )
            .await
        }
        SmokeCommand::ConsoleAppsBrowserVerify {
            installation,
            chrome_cdp_url,
            evidence_root,
            timeout_seconds,
        } => {
            let installation = InstalledTarget::load(&installation)?;
            verify_console_apps(
                installation.public_base(),
                &chrome_cdp_url,
                &evidence_root,
                Duration::from_secs(timeout_seconds),
            )
            .await
        }
        SmokeCommand::ConsoleArtifactUploadVerify {
            installation,
            chrome_cdp_url,
            evidence_root,
            bytes,
            timeout_seconds,
            preflight_only,
        } => {
            let installation = InstalledTarget::load(&installation)?;
            browser::artifact_upload::verify(
                installation.public_base(),
                &chrome_cdp_url,
                &evidence_root,
                bytes,
                Duration::from_secs(timeout_seconds),
                preflight_only,
            )
            .await
        }
        SmokeCommand::ConsoleArtifactUploadUxVerify {
            installation,
            chrome_cdp_url,
            completed_evidence,
            evidence_root,
        } => {
            let installation = InstalledTarget::load(&installation)?;
            browser::artifact_upload::ux::verify(
                installation.public_base(),
                &chrome_cdp_url,
                &completed_evidence,
                &evidence_root,
            )
            .await
        }
        SmokeCommand::ConsoleArtifactUploadResume {
            installation,
            chrome_cdp_url,
            evidence_directory,
            timeout_seconds,
        } => {
            let installation = InstalledTarget::load(&installation)?;
            browser::artifact_upload::resume::verify_resume(
                installation.public_base(),
                &chrome_cdp_url,
                &evidence_directory,
                Duration::from_secs(timeout_seconds),
            )
            .await
        }
        SmokeCommand::UavShowcaseBrowserVerify {
            conformance_bin,
            scenario,
            installation,
            chrome_cdp_url,
            evidence_root,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

            let installation = InstalledTarget::load(&installation)?;
            verify_running_showcase(
                &conformance_bin,
                &scenario,
                &installation,
                &chrome_cdp_url,
                &evidence_root,
            )
            .await
        }
        SmokeCommand::UavAgentInstructionBrowserVerify {
            installation,
            chrome_cdp_url,
            agent_id,
            message,
            timeout_seconds,
            evidence_root,
        } => {
            let installation = InstalledTarget::load(&installation)?;
            verify_uav_agent_instruction(
                installation.public_base(),
                &chrome_cdp_url,
                &agent_id,
                &message,
                Duration::from_secs(timeout_seconds),
                &evidence_root,
            )
            .await
        }
        SmokeCommand::UavShowcaseLiveRestartVerify {
            conformance_bin,
            scenario,
            installation,
            chrome_cdp_url,
            restart_timeout_seconds,
            evidence_root,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

            let installation = InstalledTarget::load(&installation)?;
            verify_live_view_restarts(RestartVerification {
                conformance: &conformance_bin,
                scenario_path: &scenario,
                installation: &installation,
                chrome_cdp_url: &chrome_cdp_url,
                restart_timeout: Duration::from_secs(restart_timeout_seconds),
                evidence_root: &evidence_root,
            })
            .await
        }
        SmokeCommand::UavRecordingBrowserVerify {
            conformance_bin,
            scenario,
            installation,
            chrome_cdp_url,
            evidence_root,
        } => {
            let conformance_bin = veoveo_testing_support::artifacts::requested_executable(
                conformance_bin,
                "veoveo-mcp-conformance",
                "conformance",
            )?;

            let installation = InstalledTarget::load(&installation)?;
            verify_running_recording(
                &conformance_bin,
                &scenario,
                &installation,
                &chrome_cdp_url,
                &evidence_root,
            )
            .await
        }
        SmokeCommand::UavRecordingArchiveBrowserVerify {
            recording_id,
            installation,
            chrome_cdp_url,
            evidence_root,
        } => {
            let installation = InstalledTarget::load(&installation)?;
            verify_recording_archive(
                recording_id,
                installation.public_base(),
                &chrome_cdp_url,
                &evidence_root,
            )
            .await
        }
    }
}

async fn verify_map_workspace(
    chrome_cdp_url: &str,
    app_html: &Path,
    evidence_root: &Path,
    timeout: Duration,
) -> Result<()> {
    let source_revision = git_revision()?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating Map workspace browser evidence directory {}",
            evidence_directory.display()
        )
    })?;
    let workspace = capture_map_workspace_app(
        chrome_cdp_url,
        app_html,
        &evidence_directory.join("map-workspace.png"),
        timeout,
    )
    .await?;
    let evidence = MapWorkspaceBrowserAcceptanceEvidence {
        schema: "veoveo.ai/map-workspace-browser-evidence/v3",
        completed_at: Utc::now(),
        source_revision,
        run_id,
        workspace,
    };
    let manifest = evidence_directory.join("evidence.json");
    fs::write(&manifest, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("writing Map workspace evidence {}", manifest.display()))?;
    println!(
        "Map workspace rendered its persistent hardware WebGL2 map and completed bounded authored and source-release viewport queries. Evidence: {}",
        manifest.display()
    );
    Ok(())
}

async fn verify_console_apps(
    public_base_url: &str,
    chrome_cdp_url: &str,
    evidence_root: &Path,
    timeout: Duration,
) -> Result<()> {
    let public_base_url = public_base_url.trim_end_matches('/');
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "Console Apps acceptance requires public HTTPS"
    );
    let source_revision = git_revision()?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating Console Apps browser evidence directory {}",
            evidence_directory.display()
        )
    })?;
    let catalog = capture_console_apps_catalog(
        chrome_cdp_url,
        public_base_url,
        &FIRST_PARTY_CONSOLE_APPS,
        &evidence_directory,
        timeout,
    )
    .await?;
    let evidence = ConsoleAppsBrowserAcceptanceEvidence {
        schema: "veoveo.ai/console-apps-browser-acceptance/v1",
        completed_at: Utc::now(),
        source_revision,
        run_id,
        catalog,
    };
    let manifest = evidence_directory.join("evidence.json");
    fs::write(&manifest, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("writing Console Apps evidence {}", manifest.display()))?;
    println!(
        "Console projected the complete grouped first-party App catalog and rendered all {} expected Apps through headed hardware graphics. Evidence: {}",
        FIRST_PARTY_CONSOLE_APPS.len(),
        manifest.display()
    );
    Ok(())
}

async fn verify_live_map_workspace(
    public_base_url: &str,
    composition_title: &str,
    layer_title: &str,
    chrome_cdp_url: &str,
    evidence_root: &Path,
    timeout: Duration,
) -> Result<()> {
    let public_base_url = public_base_url.trim_end_matches('/');
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "live Map workspace acceptance requires public HTTPS"
    );
    ensure!(
        !composition_title.trim().is_empty() && !layer_title.trim().is_empty(),
        "live Map workspace acceptance requires exact composition and layer titles"
    );
    let source_revision = git_revision()?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating live Map workspace evidence directory {}",
            evidence_directory.display()
        )
    })?;
    let workspace = capture_console_map_workspace_app(
        chrome_cdp_url,
        public_base_url,
        composition_title,
        layer_title,
        &evidence_directory,
        timeout,
    )
    .await?;
    let evidence = MapWorkspaceLiveBrowserAcceptanceEvidence {
        schema: "veoveo.ai/map-workspace-live-browser-evidence/v3",
        completed_at: Utc::now(),
        source_revision,
        run_id,
        workspace,
    };
    let manifest = evidence_directory.join("evidence.json");
    fs::write(&manifest, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("writing live Map workspace evidence {}", manifest.display()))?;
    println!(
        "The public Console rendered and interacted with the persistent map, authored and source previews, every guided workflow, and governed-data inspector through hardware WebGL2. Evidence: {}",
        manifest.display()
    );
    Ok(())
}

async fn verify_uav_agent_instruction(
    public_base_url: &str,
    chrome_cdp_url: &str,
    agent_id: &str,
    message: &str,
    timeout: Duration,
    evidence_root: &Path,
) -> Result<()> {
    let public_base_url = public_base_url.trim_end_matches('/');
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "UAV agent instruction acceptance requires public HTTPS"
    );
    ensure!(
        !agent_id.trim().is_empty() && !message.trim().is_empty(),
        "UAV agent instruction requires an exact agent and non-empty message"
    );
    let source_revision = git_revision()?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating UAV agent instruction evidence directory {}",
            evidence_directory.display()
        )
    })?;
    let instruction = send_console_uav_agent_instruction(
        chrome_cdp_url,
        public_base_url,
        agent_id,
        message,
        &evidence_directory.join("uav-agent-instruction.png"),
        timeout,
    )
    .await?;
    let evidence = AgentInstructionBrowserAcceptanceEvidence {
        schema: "veoveo.ai/uav-agent-instruction-browser-evidence/v2",
        completed_at: Utc::now(),
        source_revision,
        run_id,
        instruction,
    };
    let manifest = evidence_directory.join("evidence.json");
    fs::write(&manifest, serde_json::to_vec_pretty(&evidence)?).with_context(|| {
        format!(
            "writing UAV agent instruction evidence {}",
            manifest.display()
        )
    })?;
    println!(
        "UAV App instruction reached its pilot and received a durable reply. Evidence: {}",
        manifest.display()
    );
    Ok(())
}

async fn verify_uav_app_hosts(
    public_base_url: &str,
    chrome_cdp_url: &str,
    timeout: Duration,
) -> Result<()> {
    let public_base_url = public_base_url.trim_end_matches('/');
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "focused UAV App host acceptance requires public HTTPS"
    );
    preflight_focused_uav_app_hosts(chrome_cdp_url, public_base_url, timeout).await?;
    println!(
        "Focused UAV App host acceptance passed for the authenticated Console and standalone routes"
    );
    Ok(())
}

async fn verify_recording_archive(
    recording_id: RecordingId,
    public_base_url: &str,
    chrome_cdp_url: &str,
    evidence_root: &Path,
) -> Result<()> {
    let public_base_url = public_base_url.trim_end_matches('/');
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "focused archive browser acceptance requires public HTTPS"
    );
    let source_revision = git_revision()?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating recording archive evidence directory {}",
            evidence_directory.display()
        )
    })?;
    let recording = capture_console_recording_archive(
        chrome_cdp_url,
        public_base_url,
        recording_id,
        &evidence_directory.join("recording-archive.png"),
        Duration::from_secs(300),
    )
    .await?;
    let evidence = RecordingArchiveBrowserAcceptanceEvidence {
        schema: "veoveo.ai/uav-recording-archive-browser-evidence/v2",
        completed_at: Utc::now(),
        source_revision,
        run_id,
        recording_id,
        recording,
    };
    let manifest = evidence_directory.join("evidence.json");
    fs::write(&manifest, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("writing recording archive evidence {}", manifest.display()))?;
    println!(
        "Focused recording archive browser acceptance passed. Evidence: {}",
        manifest.display()
    );
    Ok(())
}

async fn verify_running_recording(
    conformance: &Path,
    scenario_path: &Path,
    installation: &InstalledTarget,
    chrome_cdp_url: &str,
    evidence_root: &Path,
) -> Result<()> {
    ensure!(
        conformance.is_file(),
        "required binary does not exist: {}",
        conformance.display()
    );
    let scenario: FocusedScenario = serde_json::from_slice(
        &fs::read(scenario_path)
            .with_context(|| format!("reading scenario {}", scenario_path.display()))?,
    )
    .with_context(|| format!("decoding scenario {}", scenario_path.display()))?;
    let public_base_url = installation.public_base();
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "focused browser acceptance requires public HTTPS"
    );
    let token = installation.token().await?;
    let operator = OperatorClient {
        conformance,
        installation,
        token: &token,
    };
    let initial_state = simulation_state(&operator, &scenario.session_id).await?;
    ensure!(
        json_string(&initial_state, "/lifecycle")? == "running",
        "recording browser acceptance requires the simulation to remain running: {initial_state}"
    );
    let recording_id = recording_id(&initial_state)?;
    let source_revision = git_revision()?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating recording browser evidence directory {}",
            evidence_directory.display()
        )
    })?;
    let recording = capture_console_recording(
        chrome_cdp_url,
        public_base_url,
        recording_id,
        &evidence_directory.join("recording.png"),
        Duration::from_secs(scenario.view.timeout_seconds),
    )
    .await?;
    let source_alignment = sample_source_alignment(
        source_timeline_sample(&initial_state)?,
        recording.captured_at(),
        SOURCE_SAMPLE_TIMEOUT,
        || async {
            source_timeline_sample(&simulation_state(&operator, &scenario.session_id).await?)
        },
    )
    .await?;
    let source_simulation_time_seconds = source_alignment.aligned_simulation_time_seconds;
    let recording_simulation_time_seconds = recording.final_timeline_seconds();
    let recording_source_lag_seconds =
        source_simulation_time_seconds - recording_simulation_time_seconds;
    ensure!(
        (0.0..=MAX_RECORDING_SOURCE_LAG_SECONDS).contains(&recording_source_lag_seconds),
        "live Rerun playback is not current with its simulation source: source={source_simulation_time_seconds:.3}s recording={recording_simulation_time_seconds:.3}s lag={recording_source_lag_seconds:.3}s"
    );
    let evidence = RecordingBrowserAcceptanceEvidence {
        schema: "veoveo.ai/uav-recording-browser-evidence/v2",
        completed_at: Utc::now(),
        source_revision,
        run_id,
        scenario_path: scenario_path.display().to_string(),
        session_id: scenario.session_id,
        recording_id,
        source_simulation_time_seconds,
        recording_simulation_time_seconds,
        recording_source_lag_seconds,
        source_alignment,
        recording,
    };
    let manifest = evidence_directory.join("evidence.json");
    fs::write(&manifest, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("writing recording browser evidence {}", manifest.display()))?;
    println!(
        "Focused recording browser acceptance passed without restarting or commanding the simulation. Evidence: {}",
        manifest.display()
    );
    Ok(())
}

async fn verify_running_showcase(
    conformance: &Path,
    scenario_path: &Path,
    installation: &InstalledTarget,
    chrome_cdp_url: &str,
    evidence_root: &Path,
) -> Result<()> {
    ensure!(
        conformance.is_file(),
        "required binary does not exist: {}",
        conformance.display()
    );
    let scenario: FocusedScenario = serde_json::from_slice(
        &fs::read(scenario_path)
            .with_context(|| format!("reading scenario {}", scenario_path.display()))?,
    )
    .with_context(|| format!("decoding scenario {}", scenario_path.display()))?;
    let public_base_url = installation.public_base();
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "focused browser acceptance requires public HTTPS"
    );
    let timeout = Duration::from_secs(scenario.view.timeout_seconds);
    preflight_focused_uav_app_hosts(chrome_cdp_url, public_base_url, timeout).await?;
    let token = installation.token().await?;
    let operator = OperatorClient {
        conformance,
        installation,
        token: &token,
    };
    let initial_state = simulation_state(&operator, &scenario.session_id).await?;
    ensure!(
        json_string(&initial_state, "/lifecycle")? == "running",
        "focused browser acceptance requires the existing simulation to remain running: {initial_state}"
    );
    let cameras = initial_state
        .get("live_cameras")
        .and_then(Value::as_array)
        .context("authoritative simulator omitted its live camera collection")?;
    for camera_id in QUALIFIED_CAMERA_IDS {
        let camera = cameras
            .iter()
            .find(|camera| camera.get("cameraId").and_then(Value::as_str) == Some(camera_id))
            .with_context(|| format!("running showcase omitted qualified camera {camera_id}"))?;
        ensure!(
            camera.get("health").and_then(Value::as_str) == Some("healthy"),
            "qualified camera {camera_id} is not healthy: {camera}"
        );
        ensure!(
            camera.get("streamProductId").is_none(),
            "logical camera retained a physical stream-product identity: {camera}"
        );
    }
    let primary_camera = cameras
        .iter()
        .find(|camera| camera.get("cameraId").and_then(Value::as_str) == Some(PRIMARY_CAMERA_ID))
        .context("running showcase has no primary follow camera")?;
    ensure!(
        primary_camera
            .pointer("/rig/targetEntityId")
            .and_then(Value::as_str)
            == Some(scenario.vehicle_id.as_str()),
        "primary camera does not follow the scenario vehicle: {primary_camera}"
    );
    let initial_products = initial_state
        .get("stream_products")
        .and_then(Value::as_array)
        .context("authoritative simulator omitted its tiled camera product")?;
    let initial_camera_ids = initial_products
        .iter()
        .flat_map(|product| {
            product
                .get("cameraRegions")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|region| region.get("cameraId").and_then(Value::as_str))
        })
        .collect::<std::collections::BTreeSet<_>>();
    ensure!(
        initial_products.len() == 1
            && initial_camera_ids.len() == QUALIFIED_CAMERA_IDS.len()
            && initial_products.iter().all(|product| {
                product.get("lifecycle").and_then(Value::as_str) == Some("ready")
                    && product.get("nvencSessions").and_then(Value::as_u64) == Some(1)
                    && product.get("codedWidthPx").and_then(Value::as_u64) == Some(3_840)
                    && product.get("codedHeightPx").and_then(Value::as_u64) == Some(1_440)
            }),
        "focused browser acceptance requires one ready five-camera tiled product: {initial_products:?}"
    );

    let source_revision = git_revision()?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating focused browser evidence directory {}",
            evidence_directory.display()
        )
    })?;
    let (first_live, second_live) = capture_console_live_app_pair(
        chrome_cdp_url,
        public_base_url,
        PRIMARY_CAMERA_ID,
        &evidence_directory.join("uav-live-view-first.png"),
        &evidence_directory.join("uav-live-view-second.png"),
        timeout,
    )
    .await?;
    ensure!(
        first_live.viewer_instance_id() != second_live.viewer_instance_id()
            && first_live.live_view_id() != second_live.live_view_id()
            && first_live.stream_product_id() == second_live.stream_product_id(),
        "simultaneous browser instances did not share one native camera atlas: \
         first=({}, {}, {}) second=({}, {}, {})",
        first_live.viewer_instance_id(),
        first_live.live_view_id(),
        first_live.stream_product_id(),
        second_live.viewer_instance_id(),
        second_live.live_view_id(),
        second_live.stream_product_id(),
    );
    let mut live_views = vec![first_live, second_live];
    let grid = capture_console_live_app_grid(
        chrome_cdp_url,
        public_base_url,
        &QUALIFIED_CAMERA_IDS,
        &evidence_directory.join("uav-live-view-grid.png"),
        timeout,
    )
    .await?;
    for camera_id in QUALIFIED_CAMERA_IDS.iter().skip(1) {
        live_views.push(
            capture_console_live_app(
                chrome_cdp_url,
                public_base_url,
                camera_id,
                &evidence_directory.join(format!("uav-live-view-{camera_id}.png")),
                timeout,
            )
            .await
            .with_context(|| format!("qualifying authoritative camera {camera_id}"))?,
        );
    }
    for camera_id in QUALIFIED_CAMERA_IDS {
        let expected_captures = if camera_id == PRIMARY_CAMERA_ID { 2 } else { 1 };
        ensure!(
            live_views
                .iter()
                .filter(|capture| capture.camera_id() == camera_id)
                .count()
                == expected_captures,
            "focused browser evidence omitted qualified camera {camera_id}: expected {expected_captures} captures"
        );
    }
    let concurrent_users = capture_console_live_app_five_user_grid(
        chrome_cdp_url,
        public_base_url,
        &QUALIFIED_CAMERA_IDS,
        &evidence_directory,
        timeout,
    )
    .await?;
    ensure!(
        concurrent_users.len() == 5
            && concurrent_users
                .iter()
                .all(|user| user.products().len() == 1),
        "five concurrent browser users did not each share one atlas across all five cameras"
    );
    let final_state = simulation_state(&operator, &scenario.session_id).await?;
    let final_products = final_state
        .get("stream_products")
        .and_then(Value::as_array)
        .context("authoritative simulator lost its tiled camera product")?;
    for live in &live_views {
        let product = final_products.iter().find(|product| {
            product.get("streamProductId").and_then(Value::as_str) == Some(live.stream_product_id())
        });
        ensure!(
            product.is_some_and(|product| {
                product.get("lifecycle").and_then(Value::as_str) == Some("ready")
                    && product.get("nvencSessions").and_then(Value::as_u64) == Some(1)
            }),
            "browser close disrupted tiled camera product {}: {final_products:?}",
            live.stream_product_id(),
        );
    }
    for stream_product_id in grid.products() {
        let product = final_products.iter().find(|product| {
            product.get("streamProductId").and_then(Value::as_str)
                == Some(stream_product_id.as_str())
        });
        ensure!(
            product.is_some_and(|product| {
                product.get("lifecycle").and_then(Value::as_str) == Some("ready")
                    && product.get("nvencSessions").and_then(Value::as_u64) == Some(1)
            }),
            "browser grid close disrupted tiled camera product {stream_product_id}: {final_products:?}",
        );
    }
    ensure!(
        json_string(&final_state, "/lifecycle")? == "running",
        "focused browser acceptance altered the running simulation: {final_state}"
    );
    let source_window = source_timeline_window(&initial_state, &final_state)?;
    let sensor_isolation = sensor_isolation(
        &initial_state,
        &final_state,
        &source_window,
        &scenario.vehicle_id,
    )?;
    let performance = live_view_performance(&source_window, &live_views)?;
    let evidence = BrowserAcceptanceEvidence {
        schema: EVIDENCE_SCHEMA,
        completed_at: Utc::now(),
        source_revision,
        run_id,
        scenario_path: scenario_path.display().to_string(),
        session_id: scenario.session_id,
        camera_ids: QUALIFIED_CAMERA_IDS
            .iter()
            .map(|camera_id| (*camera_id).to_owned())
            .collect(),
        source_window,
        sensor_isolation,
        performance,
        grid,
        concurrent_users,
        live_views,
    };
    let manifest = evidence_directory.join("evidence.json");
    fs::write(&manifest, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("writing focused browser evidence {}", manifest.display()))?;
    println!(
        "Focused browser acceptance passed without restarting or commanding the simulation. Evidence: {}",
        manifest.display()
    );
    Ok(())
}

async fn preflight_focused_uav_app_hosts(
    chrome_cdp_url: &str,
    public_base_url: &str,
    timeout: Duration,
) -> Result<()> {
    for host in FOCUSED_UAV_APP_HOST_PREFLIGHTS {
        match host {
            FocusedUavAppHostPreflight::Console => {
                preflight_console_live_app(chrome_cdp_url, public_base_url, timeout).await
            }
            FocusedUavAppHostPreflight::Standalone => {
                preflight_standalone_live_app(chrome_cdp_url, public_base_url, timeout).await
            }
        }
        .with_context(|| format!("preflighting the focused {} UAV App host", host.label()))?;
    }
    Ok(())
}

fn live_view_performance(
    source_window: &SourceTimelineWindowEvidence,
    live_views: &[ConsoleLiveCaptureEvidence],
) -> Result<LiveViewPerformanceEvidence> {
    let wall_seconds = (source_window.after.updated_at - source_window.before.updated_at)
        .num_nanoseconds()
        .context("source timeline performance window exceeds supported duration")?
        as f64
        / 1_000_000_000.0;
    let simulation_seconds =
        source_window.after.simulation_time_seconds - source_window.before.simulation_time_seconds;
    ensure!(
        wall_seconds > 0.0 && simulation_seconds > 0.0,
        "source timeline performance window did not advance"
    );
    let physics_real_time_factor = simulation_seconds / wall_seconds;
    ensure!(
        physics_real_time_factor >= MINIMUM_PHYSICS_REAL_TIME_FACTOR,
        "authoritative simulation real-time factor {physics_real_time_factor:.4} is below the required {MINIMUM_PHYSICS_REAL_TIME_FACTOR:.2}"
    );
    let minimum_observed_frame_rate_hz = live_views
        .iter()
        .map(ConsoleLiveCaptureEvidence::observed_frame_rate_hz)
        .fold(f64::INFINITY, f64::min);
    let maximum_observed_frame_rate_hz = live_views
        .iter()
        .map(ConsoleLiveCaptureEvidence::observed_frame_rate_hz)
        .fold(f64::NEG_INFINITY, f64::max);
    ensure!(
        minimum_observed_frame_rate_hz.is_finite() && maximum_observed_frame_rate_hz.is_finite(),
        "authoritative camera cadence evidence was empty"
    );
    Ok(LiveViewPerformanceEvidence {
        physics_real_time_factor,
        qualified_camera_count: QUALIFIED_CAMERA_IDS.len(),
        browser_user_count: 5,
        simultaneous_camera_views: 25,
        simultaneous_encoded_streams: 5,
        minimum_observed_frame_rate_hz,
        maximum_observed_frame_rate_hz,
        browser_dropped_frames: live_views
            .iter()
            .map(ConsoleLiveCaptureEvidence::cadence_dropped_frames)
            .sum(),
        maximum_source_to_render_p95_ms: live_views
            .iter()
            .map(ConsoleLiveCaptureEvidence::source_to_render_p95_ms)
            .fold(f64::NEG_INFINITY, f64::max),
        maximum_composed_motion_to_photon_upper_bound_p95_ms: live_views
            .iter()
            .map(ConsoleLiveCaptureEvidence::composed_motion_to_photon_upper_bound_p95_ms)
            .fold(f64::NEG_INFINITY, f64::max),
    })
}

fn sensor_isolation(
    before: &Value,
    after: &Value,
    source_window: &SourceTimelineWindowEvidence,
    vehicle_id: &str,
) -> Result<SensorIsolationEvidence> {
    let before_camera = physical_sensor(before, vehicle_id)?;
    let after_camera = physical_sensor(after, vehicle_id)?;
    let declared_frame_rate_hz = before_camera
        .get("frameRateHz")
        .and_then(Value::as_f64)
        .context("physical sensor omitted frame_rate_hz")?;
    ensure!(
        after_camera.get("frameRateHz").and_then(Value::as_f64) == Some(declared_frame_rate_hz)
            && before_camera.get("encoder").and_then(Value::as_str) == Some("nvidia_nvenc")
            && after_camera.get("encoder").and_then(Value::as_str) == Some("nvidia_nvenc"),
        "viewer activity changed the physical sensor contract: before={before_camera} after={after_camera}"
    );
    let frames_before = before_camera
        .get("framesObserved")
        .and_then(Value::as_u64)
        .context("physical sensor omitted frames_observed")?;
    let frames_after = after_camera
        .get("framesObserved")
        .and_then(Value::as_u64)
        .context("physical sensor omitted frames_observed")?;
    let simulation_seconds =
        source_window.after.simulation_time_seconds - source_window.before.simulation_time_seconds;
    ensure!(
        declared_frame_rate_hz > 0.0 && simulation_seconds > 0.0 && frames_after > frames_before,
        "physical sensor did not advance while viewer products were active"
    );
    let observed_frame_rate_hz = (frames_after - frames_before) as f64 / simulation_seconds;
    let minimum = declared_frame_rate_hz * 0.90;
    let maximum = declared_frame_rate_hz * 1.10;
    ensure!(
        (minimum..=maximum).contains(&observed_frame_rate_hz),
        "viewer activity changed physical sensor cadence: declared={declared_frame_rate_hz:.3}Hz observed={observed_frame_rate_hz:.3}Hz"
    );
    Ok(SensorIsolationEvidence {
        vehicle_id: vehicle_id.to_owned(),
        declared_frame_rate_hz,
        frames_before,
        frames_after,
        simulation_seconds,
        observed_frame_rate_hz,
    })
}

fn physical_sensor<'a>(state: &'a Value, vehicle_id: &str) -> Result<&'a Value> {
    state
        .get("cameras")
        .and_then(Value::as_array)
        .and_then(|cameras| {
            cameras
                .iter()
                .find(|camera| camera.get("vehicleId").and_then(Value::as_str) == Some(vehicle_id))
        })
        .with_context(|| format!("simulation state omitted physical sensor for {vehicle_id}"))
}

async fn simulation_state(operator: &OperatorClient<'_>, session_id: &str) -> Result<Value> {
    let mut last_error = None;
    for attempt in 1..=3 {
        match operator
            .call_tool(
                "uav-sim__get_simulation_state",
                serde_json::json!({"sessionId": session_id}),
            )
            .await
        {
            Ok(state) => return Ok(state),
            Err(error) if attempt < 3 => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.context("UAV state read exhausted its retry budget")?)
}

async fn gateway_conformance(
    conformance: &Path,
    installation: &InstalledTarget,
    token: &str,
    operation: &[&str],
    timeout: Duration,
) -> Result<String> {
    let mut command = tokio::process::Command::new(conformance);
    command
        .args([
            "--url",
            installation.operator.resource.as_str(),
            "--scheme",
            "uav-sim",
        ])
        .args(operation)
        .env_remove("VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")
        .env("MCP_BEARER_TOKEN", token)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(
        timeout,
        veoveo_testing_support::output_async(command, timeout),
    )
    .await
    .with_context(|| format!("conformance operation {operation:?} timed out"))??;
    ensure!(
        output.status.success(),
        "conformance operation {operation:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).context("decoding conformance output")
}

fn structured_output(output: &str) -> Result<Value> {
    let encoded = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .with_context(|| format!("conformance output omitted structured content:\n{output}"))?;
    serde_json::from_str(encoded).context("decoding structured MCP output")
}

fn json_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("JSON output omitted string {pointer}: {value}"))
}

fn recording_id(state: &Value) -> Result<RecordingId> {
    let recording: veoveo_uav_sim_mcp::contract::RecordingState = serde_json::from_value(
        state
            .pointer("/recordings/0")
            .context("simulation state omitted recording")?
            .clone(),
    )?;
    recording
        .catalog
        .recording_id()
        .context("simulation recording catalog is not ready")
}

fn git_revision() -> Result<String> {
    let output = Command::new("git").args(["rev-parse", "HEAD"]).output()?;
    if !output.status.success() {
        bail!(
            "git revision lookup failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focused_uav_acceptance_preflights_both_app_hosts() {
        assert_eq!(focused_uav_app_host_preflights(), ["console", "standalone"]);
    }

    #[test]
    fn composed_console_acceptance_covers_the_complete_first_party_catalog() {
        assert_eq!(FIRST_PARTY_CONSOLE_APPS.len(), 16);
        assert!(
            FIRST_PARTY_CONSOLE_APPS
                .iter()
                .any(|app| app.resource_uri == "ui://charts/composer.html")
        );
        assert!(
            FIRST_PARTY_CONSOLE_APPS
                .iter()
                .any(|app| app.resource_uri == "ui://datasheet/workbench.html")
        );
    }

    #[test]
    fn live_view_window_preserves_the_declared_sensor_cadence() {
        let before = serde_json::json!({
            "lifecycle": "running",
            "simulation_time_s": 100.0,
            "updated_at": "2026-08-05T12:00:00Z",
            "cameras": [{
                "vehicleId": "uav-1",
                "frameRateHz": 2,
                "framesObserved": 500,
                "encoder": "nvidia_nvenc"
            }]
        });
        let after = serde_json::json!({
            "lifecycle": "running",
            "simulation_time_s": 120.0,
            "updated_at": "2026-08-05T12:00:20Z",
            "cameras": [{
                "vehicleId": "uav-1",
                "frameRateHz": 2,
                "framesObserved": 540,
                "encoder": "nvidia_nvenc"
            }]
        });
        let window = source_timeline_window(&before, &after).unwrap();

        let evidence = sensor_isolation(&before, &after, &window, "uav-1").unwrap();

        assert_eq!(evidence.observed_frame_rate_hz, 2.0);
        assert_eq!(evidence.frames_after - evidence.frames_before, 40);
    }

    #[test]
    fn live_view_window_rejects_sensor_cadence_coupled_to_operator_video() {
        let before = serde_json::json!({
            "lifecycle": "running",
            "simulation_time_s": 100.0,
            "updated_at": "2026-08-05T12:00:00Z",
            "cameras": [{
                "vehicleId": "uav-1",
                "frameRateHz": 2,
                "framesObserved": 500,
                "encoder": "nvidia_nvenc"
            }]
        });
        let after = serde_json::json!({
            "lifecycle": "running",
            "simulation_time_s": 110.0,
            "updated_at": "2026-08-05T12:00:10Z",
            "cameras": [{
                "vehicleId": "uav-1",
                "frameRateHz": 2,
                "framesObserved": 800,
                "encoder": "nvidia_nvenc"
            }]
        });
        let window = source_timeline_window(&before, &after).unwrap();

        assert!(sensor_isolation(&before, &after, &window, "uav-1").is_err());
    }
}
