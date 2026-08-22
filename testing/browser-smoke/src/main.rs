use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[allow(dead_code)]
#[path = "../../smoke/src/bin/smoke/scenarios/uav_sim/browser.rs"]
mod browser;
mod restart;

use browser::{
    ConsoleLiveCaptureEvidence, ConsoleLiveGridEvidence, ConsoleRecordingArchiveCaptureEvidence,
    ConsoleRecordingCaptureEvidence, capture_console_live_app,
    capture_console_live_app_five_user_grid, capture_console_live_app_grid,
    capture_console_live_app_pair, capture_console_recording, capture_console_recording_archive,
    preflight_console_live_app, preflight_standalone_live_app,
};
use restart::{RestartVerification, verify_live_view_restarts};

const EVIDENCE_SCHEMA: &str = "veoveo.io/uav-live-view-browser-evidence/v12";
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
const OPERATOR_PROFILE_SCOPES: &[&str] = &[
    "operator:use",
    "uav-sim:read",
    "uav-sim:control",
    "uav-sim:stream",
    "view:read",
    "view:write",
    "view:capture",
    "map:dataset:read",
    "map:route",
    "time:read",
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

#[derive(Debug, Parser)]
#[command(
    name = "browser-smoke",
    about = "Focused headed-browser acceptance against a running VeoVeo installation"
)]
struct Args {
    #[command(subcommand)]
    command: SmokeCommand,
}

#[derive(Debug, Subcommand)]
// These are deliberately full, stable xtask dispatch names in a focused UAV
// browser binary; the shared prefix is part of the CLI rather than Rust type noise.
#[allow(clippy::enum_variant_names)]
enum SmokeCommand {
    /// Verify the Console and standalone UAV App hosts without opening live products.
    UavAppHostsBrowserVerify {
        #[arg(long)]
        public_base_url: String,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value_t = 180)]
        timeout_seconds: u64,
    },
    /// Repeat headed Console acceptance without restarting or commanding the simulation.
    UavShowcaseBrowserVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        #[arg(long)]
        public_base_url: String,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value = "output/acceptance/uav-browser")]
        evidence_root: PathBuf,
    },
    /// Keep a headed live view mounted while restarting the MCP and simulator containers.
    UavShowcaseLiveRestartVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        #[arg(long)]
        context: String,
        #[arg(long, default_value = "veoveo")]
        namespace: String,
        #[arg(long)]
        public_base_url: String,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value_t = 1_800)]
        restart_timeout_seconds: u64,
        #[arg(long, default_value = "output/acceptance/uav-live-restart")]
        evidence_root: PathBuf,
    },
    /// Verify only governed live Recording playback against the running source.
    UavRecordingBrowserVerify {
        #[arg(long, default_value = "target/debug/conformance")]
        conformance_bin: PathBuf,
        #[arg(
            long,
            default_value = "showcase/uav-sim/scenarios/new-york-aerial.json"
        )]
        scenario: PathBuf,
        #[arg(long)]
        public_base_url: String,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(long, default_value = "output/acceptance/uav-recording-browser")]
        evidence_root: PathBuf,
    },
    /// Verify one governed sealed Recording through the lazy Redap archive path.
    UavRecordingArchiveBrowserVerify {
        #[arg(long)]
        recording_id: String,
        #[arg(long)]
        public_base_url: String,
        #[arg(long, default_value = "http://127.0.0.1:9222")]
        chrome_cdp_url: String,
        #[arg(
            long,
            default_value = "output/acceptance/uav-recording-archive-browser"
        )]
        evidence_root: PathBuf,
    },
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
    recording_id: String,
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
    recording_id: String,
    recording: ConsoleRecordingArchiveCaptureEvidence,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceTimelineSample {
    updated_at: chrono::DateTime<Utc>,
    simulation_time_seconds: f64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceTimelineWindowEvidence {
    before: SourceTimelineSample,
    after: SourceTimelineSample,
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

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceTimelineAlignmentEvidence {
    before: SourceTimelineSample,
    after: SourceTimelineSample,
    recording_observed_at: chrono::DateTime<Utc>,
    interpolation_fraction: f64,
    aligned_simulation_time_seconds: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveViewPerformanceEvidence {
    physics_real_time_factor: f64,
    qualified_camera_count: usize,
    browser_user_count: usize,
    simultaneous_stream_count: usize,
    minimum_observed_frame_rate_hz: f64,
    maximum_observed_frame_rate_hz: f64,
    browser_dropped_frames: u64,
    maximum_source_to_render_p95_ms: f64,
    maximum_composed_motion_to_photon_upper_bound_p95_ms: f64,
}

struct OperatorClient<'a> {
    conformance: &'a Path,
    base: &'a str,
    token: &'a str,
}

impl OperatorClient<'_> {
    async fn conformance(&self, operation: &[&str], timeout: Duration) -> Result<String> {
        gateway_conformance(self.conformance, self.base, self.token, operation, timeout).await
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
    let _ = rustls::crypto::ring::default_provider().install_default();
    match Args::parse().command {
        SmokeCommand::UavAppHostsBrowserVerify {
            public_base_url,
            chrome_cdp_url,
            timeout_seconds,
        } => {
            verify_uav_app_hosts(
                &public_base_url,
                &chrome_cdp_url,
                Duration::from_secs(timeout_seconds),
            )
            .await
        }
        SmokeCommand::UavShowcaseBrowserVerify {
            conformance_bin,
            scenario,
            public_base_url,
            chrome_cdp_url,
            evidence_root,
        } => {
            verify_running_showcase(
                &conformance_bin,
                &scenario,
                &public_base_url,
                &chrome_cdp_url,
                &evidence_root,
            )
            .await
        }
        SmokeCommand::UavShowcaseLiveRestartVerify {
            conformance_bin,
            scenario,
            context,
            namespace,
            public_base_url,
            chrome_cdp_url,
            restart_timeout_seconds,
            evidence_root,
        } => {
            verify_live_view_restarts(RestartVerification {
                conformance: &conformance_bin,
                scenario_path: &scenario,
                context: &context,
                namespace: &namespace,
                public_base_url: &public_base_url,
                chrome_cdp_url: &chrome_cdp_url,
                restart_timeout: Duration::from_secs(restart_timeout_seconds),
                evidence_root: &evidence_root,
            })
            .await
        }
        SmokeCommand::UavRecordingBrowserVerify {
            conformance_bin,
            scenario,
            public_base_url,
            chrome_cdp_url,
            evidence_root,
        } => {
            verify_running_recording(
                &conformance_bin,
                &scenario,
                &public_base_url,
                &chrome_cdp_url,
                &evidence_root,
            )
            .await
        }
        SmokeCommand::UavRecordingArchiveBrowserVerify {
            recording_id,
            public_base_url,
            chrome_cdp_url,
            evidence_root,
        } => {
            verify_recording_archive(
                &recording_id,
                &public_base_url,
                &chrome_cdp_url,
                &evidence_root,
            )
            .await
        }
    }
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
    recording_id: &str,
    public_base_url: &str,
    chrome_cdp_url: &str,
    evidence_root: &Path,
) -> Result<()> {
    ensure!(
        uuid::Uuid::parse_str(recording_id)?.get_version_num() == 7,
        "archive recording identity must be UUIDv7"
    );
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
        schema: "veoveo.io/uav-recording-archive-browser-evidence/v1",
        completed_at: Utc::now(),
        source_revision,
        run_id,
        recording_id: recording_id.to_owned(),
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
    public_base_url: &str,
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
    let public_base_url = public_base_url.trim_end_matches('/');
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "focused browser acceptance requires public HTTPS"
    );
    let token = gateway_token(conformance, public_base_url).await?;
    let operator = OperatorClient {
        conformance,
        base: public_base_url,
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
        &recording_id,
        &evidence_directory.join("recording.png"),
        Duration::from_secs(scenario.view.timeout_seconds),
    )
    .await?;
    let final_state = simulation_state(&operator, &scenario.session_id).await?;
    ensure!(
        json_string(&final_state, "/lifecycle")? == "running",
        "recording browser acceptance altered the running simulation: {final_state}"
    );
    let source_alignment =
        align_source_timeline(&initial_state, &final_state, recording.captured_at())?;
    let source_simulation_time_seconds = source_alignment.aligned_simulation_time_seconds;
    let recording_simulation_time_seconds = recording.final_timeline_seconds();
    let recording_source_lag_seconds =
        source_simulation_time_seconds - recording_simulation_time_seconds;
    ensure!(
        (0.0..=MAX_RECORDING_SOURCE_LAG_SECONDS).contains(&recording_source_lag_seconds),
        "live Rerun playback is not current with its simulation source: source={source_simulation_time_seconds:.3}s recording={recording_simulation_time_seconds:.3}s lag={recording_source_lag_seconds:.3}s"
    );
    let evidence = RecordingBrowserAcceptanceEvidence {
        schema: "veoveo.io/uav-recording-browser-evidence/v2",
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
    public_base_url: &str,
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
    let public_base_url = public_base_url.trim_end_matches('/');
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "focused browser acceptance requires public HTTPS"
    );
    let timeout = Duration::from_secs(scenario.view.timeout_seconds);
    preflight_focused_uav_app_hosts(chrome_cdp_url, public_base_url, timeout).await?;
    let token = gateway_token(conformance, public_base_url).await?;
    let operator = OperatorClient {
        conformance,
        base: public_base_url,
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
        .context("authoritative simulator omitted its shared camera products")?;
    let initial_product_ids = initial_products
        .iter()
        .filter_map(|product| product.get("streamProductId").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>();
    let initial_camera_ids = initial_products
        .iter()
        .filter_map(|product| product.get("cameraId").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>();
    ensure!(
        initial_products.len() == QUALIFIED_CAMERA_IDS.len()
            && initial_product_ids.len() == QUALIFIED_CAMERA_IDS.len()
            && initial_camera_ids.len() == QUALIFIED_CAMERA_IDS.len()
            && initial_products.iter().all(|product| {
                product.get("lifecycle").and_then(Value::as_str) == Some("ready")
                    && product.get("activeViewers").and_then(Value::as_u64) == Some(0)
                    && product.get("connectedViewers").and_then(Value::as_u64) == Some(0)
                    && product.get("nvencSessions").and_then(Value::as_u64) == Some(1)
            }),
        "focused browser acceptance requires five ready shared camera products: {initial_products:?}"
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
        "simultaneous browser instances did not share one native camera product: \
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
                .all(|user| user.products().len() == QUALIFIED_CAMERA_IDS.len()),
        "five concurrent browser users did not each receive all five camera streams"
    );
    let final_state = simulation_state(&operator, &scenario.session_id).await?;
    let final_products = final_state
        .get("stream_products")
        .and_then(Value::as_array)
        .context("authoritative simulator lost its shared camera products")?;
    for live in &live_views {
        let product = final_products.iter().find(|product| {
            product.get("streamProductId").and_then(Value::as_str) == Some(live.stream_product_id())
        });
        ensure!(
            product.is_some_and(|product| {
                product.get("lifecycle").and_then(Value::as_str) == Some("ready")
                    && product.get("activeViewers").and_then(Value::as_u64) == Some(0)
                    && product.get("connectedViewers").and_then(Value::as_u64) == Some(0)
                    && product.get("nvencSessions").and_then(Value::as_u64) == Some(1)
            }),
            "browser close disrupted shared camera product {}: {final_products:?}",
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
                    && product.get("activeViewers").and_then(Value::as_u64) == Some(0)
                    && product.get("connectedViewers").and_then(Value::as_u64) == Some(0)
                    && product.get("nvencSessions").and_then(Value::as_u64) == Some(1)
            }),
            "browser grid close disrupted shared camera product {stream_product_id}: {final_products:?}",
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
        simultaneous_stream_count: 25,
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

fn source_timeline_window(before: &Value, after: &Value) -> Result<SourceTimelineWindowEvidence> {
    let before = source_timeline_sample(before)?;
    let after = source_timeline_sample(after)?;
    ensure!(
        after.updated_at > before.updated_at
            && after.simulation_time_seconds > before.simulation_time_seconds,
        "running simulation source timeline did not advance: {before:?} -> {after:?}"
    );
    Ok(SourceTimelineWindowEvidence { before, after })
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
        .get("frame_rate_hz")
        .and_then(Value::as_f64)
        .context("physical sensor omitted frame_rate_hz")?;
    ensure!(
        after_camera.get("frame_rate_hz").and_then(Value::as_f64) == Some(declared_frame_rate_hz)
            && before_camera.get("encoder").and_then(Value::as_str) == Some("nvidia_nvenc")
            && after_camera.get("encoder").and_then(Value::as_str) == Some("nvidia_nvenc"),
        "viewer activity changed the physical sensor contract: before={before_camera} after={after_camera}"
    );
    let frames_before = before_camera
        .get("frames_observed")
        .and_then(Value::as_u64)
        .context("physical sensor omitted frames_observed")?;
    let frames_after = after_camera
        .get("frames_observed")
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
                .find(|camera| camera.get("vehicle_id").and_then(Value::as_str) == Some(vehicle_id))
        })
        .with_context(|| format!("simulation state omitted physical sensor for {vehicle_id}"))
}

fn align_source_timeline(
    before: &Value,
    after: &Value,
    recording_observed_at: chrono::DateTime<Utc>,
) -> Result<SourceTimelineAlignmentEvidence> {
    let before = source_timeline_sample(before)?;
    let after = source_timeline_sample(after)?;
    ensure!(
        after.updated_at > before.updated_at
            && after.simulation_time_seconds > before.simulation_time_seconds,
        "running simulation source timeline did not advance: {before:?} -> {after:?}"
    );
    ensure!(
        (before.updated_at..=after.updated_at).contains(&recording_observed_at),
        "Rerun observation is not bracketed by source samples: before={before:?} observation={recording_observed_at} after={after:?}"
    );
    let total_nanoseconds = (after.updated_at - before.updated_at)
        .num_nanoseconds()
        .context("source timeline bracket exceeds supported duration")?;
    let observed_nanoseconds = (recording_observed_at - before.updated_at)
        .num_nanoseconds()
        .context("source timeline observation exceeds supported duration")?;
    let interpolation_fraction = observed_nanoseconds as f64 / total_nanoseconds as f64;
    let aligned_simulation_time_seconds = before.simulation_time_seconds
        + (after.simulation_time_seconds - before.simulation_time_seconds) * interpolation_fraction;
    Ok(SourceTimelineAlignmentEvidence {
        before,
        after,
        recording_observed_at,
        interpolation_fraction,
        aligned_simulation_time_seconds,
    })
}

fn source_timeline_sample(state: &Value) -> Result<SourceTimelineSample> {
    let simulation_time_seconds = state
        .get("simulation_time_s")
        .and_then(Value::as_f64)
        .context("running simulation omitted simulation_time_s")?;
    ensure!(
        simulation_time_seconds.is_finite() && simulation_time_seconds >= 0.0,
        "running simulation returned invalid simulation_time_s {simulation_time_seconds}"
    );
    let updated_at = chrono::DateTime::parse_from_rfc3339(json_string(state, "/updated_at")?)
        .context("running simulation returned invalid updated_at")?
        .with_timezone(&Utc);
    Ok(SourceTimelineSample {
        updated_at,
        simulation_time_seconds,
    })
}

async fn simulation_state(operator: &OperatorClient<'_>, session_id: &str) -> Result<Value> {
    let mut last_error = None;
    for attempt in 1..=3 {
        match operator
            .call_tool(
                "uav-sim__get_simulation_state",
                serde_json::json!({"session_id": session_id}),
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

async fn gateway_token(conformance: &Path, base: &str) -> Result<String> {
    let token_url = format!("{base}/oauth/token");
    let resource = format!("{base}/mcp/operator");
    let mut command = tokio::process::Command::new(conformance);
    command
        .args([
            "gateway-token-exchange",
            "--token-url",
            &token_url,
            "--client-id",
            "operator-service",
            "--audience",
            &token_url,
            "--resource",
            &resource,
            "--work-context",
            "operations",
        ])
        .args(
            OPERATOR_PROFILE_SCOPES
                .iter()
                .flat_map(|scope| ["--scope", *scope]),
        )
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .context("gateway token exchange timed out")??;
    ensure!(
        output.status.success(),
        "gateway token exchange failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let token = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(!token.is_empty(), "gateway returned an empty access token");
    Ok(token)
}

async fn gateway_conformance(
    conformance: &Path,
    base: &str,
    token: &str,
    operation: &[&str],
    timeout: Duration,
) -> Result<String> {
    let url = format!("{base}/mcp/operator");
    let mut command = tokio::process::Command::new(conformance);
    command
        .args(["--url", &url, "--scheme", "uav-sim"])
        .args(operation)
        .env_remove("VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")
        .env("MCP_BEARER_TOKEN", token)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(timeout, command.output())
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

fn recording_id(state: &Value) -> Result<String> {
    ensure!(
        json_string(state, "/recordings/0/catalog_lifecycle")? == "ready",
        "simulation recording has not reached the governed catalog: {}",
        state.pointer("/recordings/0").unwrap_or(&Value::Null)
    );
    let uri = json_string(state, "/recordings/0/recording_uri")?;
    let id = uri
        .strip_prefix("recording://recordings/")
        .context("simulation returned a non-canonical recording URI")?;
    ensure!(
        uuid::Uuid::parse_str(id)?.get_version_num() == 7,
        "recording identity must be UUIDv7"
    );
    Ok(id.to_owned())
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
    fn source_timeline_is_aligned_at_the_rerun_observation() {
        let before = serde_json::json!({
            "simulation_time_s": 100.0,
            "updated_at": "2026-08-05T12:00:00Z"
        });
        let after = serde_json::json!({
            "simulation_time_s": 120.0,
            "updated_at": "2026-08-05T12:00:20Z"
        });
        let observed_at = chrono::DateTime::parse_from_rfc3339("2026-08-05T12:00:07.5Z")
            .unwrap()
            .with_timezone(&Utc);

        let aligned = align_source_timeline(&before, &after, observed_at).unwrap();

        assert_eq!(aligned.interpolation_fraction, 0.375);
        assert_eq!(aligned.aligned_simulation_time_seconds, 107.5);
    }

    #[test]
    fn source_timeline_rejects_unbracketed_rerun_observation() {
        let before = serde_json::json!({
            "simulation_time_s": 100.0,
            "updated_at": "2026-08-05T12:00:00Z"
        });
        let after = serde_json::json!({
            "simulation_time_s": 120.0,
            "updated_at": "2026-08-05T12:00:20Z"
        });
        let observed_at = chrono::DateTime::parse_from_rfc3339("2026-08-05T12:00:21Z")
            .unwrap()
            .with_timezone(&Utc);

        assert!(align_source_timeline(&before, &after, observed_at).is_err());
    }

    #[test]
    fn live_view_window_preserves_the_declared_sensor_cadence() {
        let before = serde_json::json!({
            "simulation_time_s": 100.0,
            "updated_at": "2026-08-05T12:00:00Z",
            "cameras": [{
                "vehicle_id": "uav-1",
                "frame_rate_hz": 2,
                "frames_observed": 500,
                "encoder": "nvidia_nvenc"
            }]
        });
        let after = serde_json::json!({
            "simulation_time_s": 120.0,
            "updated_at": "2026-08-05T12:00:20Z",
            "cameras": [{
                "vehicle_id": "uav-1",
                "frame_rate_hz": 2,
                "frames_observed": 540,
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
            "simulation_time_s": 100.0,
            "updated_at": "2026-08-05T12:00:00Z",
            "cameras": [{
                "vehicle_id": "uav-1",
                "frame_rate_hz": 2,
                "frames_observed": 500,
                "encoder": "nvidia_nvenc"
            }]
        });
        let after = serde_json::json!({
            "simulation_time_s": 110.0,
            "updated_at": "2026-08-05T12:00:10Z",
            "cameras": [{
                "vehicle_id": "uav-1",
                "frame_rate_hz": 2,
                "frames_observed": 800,
                "encoder": "nvidia_nvenc"
            }]
        });
        let window = source_timeline_window(&before, &after).unwrap();

        assert!(sensor_isolation(&before, &after, &window, "uav-1").is_err());
    }
}
