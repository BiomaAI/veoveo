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
#[path = "../../smoke/src/bin/smoke/scenarios/simulation_view/browser.rs"]
mod browser;

use browser::{
    ConsoleLiveCaptureEvidence, ConsoleRecordingCaptureEvidence, ConsoleStreamCaptureEvidence,
    capture_console_live_app, capture_console_recording, capture_console_stream_app,
};

const EVIDENCE_SCHEMA: &str = "veoveo.io/uav-showcase-browser-evidence/v2";
const MAX_RECORDING_SOURCE_LAG_SECONDS: f64 = 1.0;
const OPERATOR_PROFILE_SCOPES: &[&str] = &[
    "operator:use",
    "simulation-view:read",
    "simulation-view:write",
    "simulation-view:stream",
    "view:read",
    "view:write",
    "view:capture",
    "map:dataset:read",
    "time:read",
];

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
enum SmokeCommand {
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
    camera_id: String,
    recording_id: String,
    initial_pose_sequence: u64,
    final_pose_sequence: u64,
    source_simulation_time_seconds: f64,
    recording_simulation_time_seconds: f64,
    recording_source_lag_seconds: f64,
    live: ConsoleLiveCaptureEvidence,
    stream: ConsoleStreamCaptureEvidence,
    recording: ConsoleRecordingCaptureEvidence,
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
    recording: ConsoleRecordingCaptureEvidence,
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
    }
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
    let source_simulation_time_seconds = final_state
        .get("simulation_time_s")
        .and_then(Value::as_f64)
        .context("running simulation omitted simulation_time_s")?;
    let recording_simulation_time_seconds = recording.final_timeline_seconds();
    let recording_source_lag_seconds =
        source_simulation_time_seconds - recording_simulation_time_seconds;
    ensure!(
        (0.0..=MAX_RECORDING_SOURCE_LAG_SECONDS).contains(&recording_source_lag_seconds),
        "live Rerun playback is not current with its simulation source: source={source_simulation_time_seconds:.3}s recording={recording_simulation_time_seconds:.3}s lag={recording_source_lag_seconds:.3}s"
    );
    let evidence = RecordingBrowserAcceptanceEvidence {
        schema: "veoveo.io/uav-recording-browser-evidence/v1",
        completed_at: Utc::now(),
        source_revision,
        run_id,
        scenario_path: scenario_path.display().to_string(),
        session_id: scenario.session_id,
        recording_id,
        source_simulation_time_seconds,
        recording_simulation_time_seconds,
        recording_source_lag_seconds,
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
    let token = gateway_token(conformance, public_base_url).await?;
    let operator = OperatorClient {
        conformance,
        base: public_base_url,
        token: &token,
    };
    let state = simulation_state(&operator, &scenario.session_id).await?;
    ensure!(
        json_string(&state, "/lifecycle")? == "running",
        "focused browser acceptance requires the existing simulation to remain running: {state}"
    );
    let recording_id = recording_id(&state)?;
    let cameras = read_json_resource(
        &operator,
        &format!("simulation-view://session/{}/cameras", scenario.session_id),
    )
    .await?;
    let camera = cameras
        .as_array()
        .context("Simulation View camera collection is not an array")?
        .iter()
        .find(|camera| {
            camera
                .pointer("/definition/rig/targetEntity")
                .and_then(Value::as_str)
                == Some(scenario.vehicle_id.as_str())
                && camera.get("health").and_then(Value::as_str) == Some("healthy")
        })
        .context("running showcase has no healthy leader follow camera")?;
    let camera_id = json_string(camera, "/cameraId")?.to_owned();
    let initial_pose_sequence = camera
        .get("lastPoseSequence")
        .and_then(Value::as_u64)
        .context("existing follow camera has no admitted pose sequence")?;

    let source_revision = git_revision()?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating focused browser evidence directory {}",
            evidence_directory.display()
        )
    })?;
    let timeout = Duration::from_secs(scenario.view.timeout_seconds);
    let live = capture_console_live_app(
        chrome_cdp_url,
        public_base_url,
        &camera_id,
        &evidence_directory.join("simulation-view.png"),
        timeout,
    )
    .await?;
    let stream = capture_console_stream_app(
        chrome_cdp_url,
        public_base_url,
        &evidence_directory.join("stream.png"),
        timeout,
    )
    .await?;
    let recording = capture_console_recording(
        chrome_cdp_url,
        public_base_url,
        &recording_id,
        &evidence_directory.join("recording.png"),
        timeout,
    )
    .await?;

    let final_camera = read_json_resource(
        &operator,
        &format!(
            "simulation-view://session/{}/camera/{camera_id}",
            scenario.session_id
        ),
    )
    .await?;
    let final_pose_sequence = final_camera
        .get("lastPoseSequence")
        .and_then(Value::as_u64)
        .context("follow camera lost its admitted pose sequence")?;
    ensure!(
        final_pose_sequence > initial_pose_sequence,
        "pose sequence did not advance during focused browser acceptance: {initial_pose_sequence} -> {final_pose_sequence}"
    );
    let final_state = simulation_state(&operator, &scenario.session_id).await?;
    ensure!(
        json_string(&final_state, "/lifecycle")? == "running",
        "focused browser acceptance altered the running simulation: {final_state}"
    );
    let source_simulation_time_seconds = final_state
        .get("simulation_time_s")
        .and_then(Value::as_f64)
        .context("running simulation omitted simulation_time_s")?;
    let recording_simulation_time_seconds = recording.final_timeline_seconds();
    let recording_source_lag_seconds =
        source_simulation_time_seconds - recording_simulation_time_seconds;
    ensure!(
        (0.0..=MAX_RECORDING_SOURCE_LAG_SECONDS).contains(&recording_source_lag_seconds),
        "live Rerun playback is not current with its simulation source: source={source_simulation_time_seconds:.3}s recording={recording_simulation_time_seconds:.3}s lag={recording_source_lag_seconds:.3}s"
    );
    let evidence = BrowserAcceptanceEvidence {
        schema: EVIDENCE_SCHEMA,
        completed_at: Utc::now(),
        source_revision,
        run_id,
        scenario_path: scenario_path.display().to_string(),
        session_id: scenario.session_id,
        camera_id,
        recording_id,
        initial_pose_sequence,
        final_pose_sequence,
        source_simulation_time_seconds,
        recording_simulation_time_seconds,
        recording_source_lag_seconds,
        live,
        stream,
        recording,
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

async fn read_json_resource(operator: &OperatorClient<'_>, uri: &str) -> Result<Value> {
    serde_json::from_str(
        &operator
            .conformance(&["resource", uri], Duration::from_secs(60))
            .await?,
    )
    .with_context(|| format!("decoding MCP resource {uri}"))
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
