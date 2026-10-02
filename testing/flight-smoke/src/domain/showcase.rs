use chrono::Utc;
use serde::Serialize;

use super::browser::{
    ConsoleLiveCaptureEvidence, ConsoleRecordingCaptureEvidence, ConsoleStreamCaptureEvidence,
    capture_console_live_app, capture_console_recording, capture_console_stream_app,
    preflight_console_live_app, preflight_standalone_live_app,
};
use super::*;

const EVIDENCE_SCHEMA: &str = "veoveo.ai/uav-showcase-acceptance-evidence/v4";
const PRIMARY_CAMERA_ID: &str = "follow";

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PhaseOutcome {
    Passed,
    Failed,
}

impl PhaseOutcome {
    fn from_result<T>(result: &Result<T>) -> Self {
        if result.is_ok() {
            Self::Passed
        } else {
            Self::Failed
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhaseOutcomes<'a> {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    source_revision: &'a str,
    run_id: &'a str,
    domain: PhaseOutcome,
    visual: PhaseOutcome,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FlightCheckpointEvidence {
    phase: FlightCheckpoint,
    captured_at: chrono::DateTime<Utc>,
    flight_state: String,
    relative_altitude_m: f64,
    native_sensor_frame_sequence: u64,
    console: ConsoleLiveCaptureEvidence,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum FlightCheckpoint {
    Takeoff,
    Mission,
    Landing,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShowcaseEvidence {
    schema: &'static str,
    completed_at: chrono::DateTime<Utc>,
    source_revision: String,
    run_id: String,
    scenario_path: String,
    session_id: veoveo_uav_sim_mcp::contract::SessionId,
    camera_id: String,
    camera_rig: &'static str,
    recording_id: RecordingId,
    checkpoints: Vec<FlightCheckpointEvidence>,
    stream: ConsoleStreamCaptureEvidence,
    recording: ConsoleRecordingCaptureEvidence,
    recording_source_latency: RecordingSourceLatencyEvidence,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingSourceLatencyEvidence {
    sampled_at: chrono::DateTime<Utc>,
    source_timeline_seconds: f64,
    viewer_timeline_seconds: f64,
    source_to_viewer_seconds: f64,
}

struct FlightEvidence {
    recording_id: RecordingId,
    checkpoints: Vec<FlightCheckpointEvidence>,
    stream: ConsoleStreamCaptureEvidence,
    recording: ConsoleRecordingCaptureEvidence,
    recording_source_latency: RecordingSourceLatencyEvidence,
}

struct VisualCaptureSignals {
    takeoff_ready: tokio::sync::oneshot::Receiver<()>,
    takeoff_complete: tokio::sync::oneshot::Sender<()>,
    mission_ready: tokio::sync::oneshot::Receiver<()>,
    stream_complete: tokio::sync::oneshot::Sender<()>,
    moving_recording_complete: tokio::sync::oneshot::Sender<()>,
}

pub(crate) async fn uav_showcase_verify(
    conformance: &Path,
    scenario_path: &Path,
    installation: &InstalledTarget,
    chrome_cdp_url: &str,
    evidence_root: &Path,
) -> Result<()> {
    let scenario = UavAcceptanceScenario::load(scenario_path)?;
    assert_executable(conformance)?;
    preflight_flight_authority(installation)?;
    let context = &installation.target.kubernetes.context;
    let namespace = &installation.target.kubernetes.namespace;
    let public_base_url = installation.public_base();
    assert_showcase_gpu_workloads(context, namespace)?;
    let operator = OperatorClient {
        conformance,
        installation,
    };
    let info = operator
        .conformance(&["info"], Duration::from_secs(60))
        .await?;
    for tool in [
        "uav-sim__list_live_cameras",
        "uav-sim__open_live_view",
        "uav-sim__renew_live_view",
        "uav-sim__close_live_view",
    ] {
        contains(&info, tool)?;
    }

    ensure_world_configured(&operator, &scenario).await?;
    wait_for_live_products(
        &operator,
        &scenario,
        PRIMARY_CAMERA_ID,
        Duration::from_secs(scenario.view.timeout_seconds),
    )
    .await?;
    preflight_console_live_app(
        chrome_cdp_url,
        public_base_url,
        Duration::from_secs(scenario.view.timeout_seconds),
    )
    .await
    .context("preflighting the authenticated Console UAV live-view App")?;
    preflight_standalone_live_app(
        chrome_cdp_url,
        public_base_url,
        Duration::from_secs(scenario.view.timeout_seconds),
    )
    .await
    .context("preflighting the authenticated standalone UAV live-view App")?;

    let source_revision = run_checked(
        Path::new("git"),
        ["rev-parse", "HEAD"].map(OsString::from),
        [],
    )?
    .trim()
    .to_owned();
    ensure!(
        source_revision.len() == 40 && source_revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "acceptance source revision was not a full Git object id"
    );
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&source_revision).join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating UAV acceptance evidence directory {}",
            evidence_directory.display()
        )
    })?;

    let (takeoff_ready, await_takeoff) = tokio::sync::oneshot::channel();
    let (takeoff_capture_complete, hold_takeoff) = tokio::sync::oneshot::channel();
    let (mission_ready, await_mission) = tokio::sync::oneshot::channel();
    let (stream_capture_complete, hold_live_stream) = tokio::sync::oneshot::channel();
    let (recording_capture_complete, hold_landing) = tokio::sync::oneshot::channel();
    let domain = uav_sim_verify_with_visual_hold(
        conformance,
        scenario_path,
        installation,
        Some(UavVisualHolds {
            phases: UavVisualPhases {
                takeoff_ready,
                takeoff_capture_complete: hold_takeoff,
                mission_ready,
            },
            stream_capture_complete: hold_live_stream,
            moving_recording_capture_complete: hold_landing,
        }),
    );
    let visual = monitor_flight(
        &operator,
        &scenario,
        chrome_cdp_url,
        public_base_url,
        PRIMARY_CAMERA_ID,
        &evidence_directory,
        VisualCaptureSignals {
            takeoff_ready: await_takeoff,
            takeoff_complete: takeoff_capture_complete,
            mission_ready: await_mission,
            stream_complete: stream_capture_complete,
            moving_recording_complete: recording_capture_complete,
        },
    );
    let (domain_result, visual_result) = tokio::join!(domain, visual);
    let flight = finish_phases(
        domain_result,
        visual_result,
        &evidence_directory,
        &source_revision,
        &run_id,
    )?;

    let evidence = ShowcaseEvidence {
        schema: EVIDENCE_SCHEMA,
        completed_at: Utc::now(),
        source_revision,
        run_id,
        scenario_path: scenario_path.display().to_string(),
        session_id: scenario.session_id.clone(),
        camera_id: PRIMARY_CAMERA_ID.to_owned(),
        camera_rig: "follow_entity",
        recording_id: flight.recording_id,
        checkpoints: flight.checkpoints,
        stream: flight.stream,
        recording: flight.recording,
        recording_source_latency: flight.recording_source_latency,
    };
    let manifest_path = evidence_directory.join("evidence.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("writing acceptance evidence {}", manifest_path.display()))?;
    println!(
        "UAV showcase acceptance ok: one authoritative simulation owned the world, camera, RTX \
         render product, and NVIDIA NVENC product throughout takeoff, mission, and landing. \
         Evidence: {}",
        manifest_path.display()
    );
    Ok(())
}

pub(crate) async fn uav_showcase_up(
    conformance: &Path,
    scenario_path: &Path,
    installation: &InstalledTarget,
) -> Result<()> {
    let scenario = UavAcceptanceScenario::load(scenario_path)?;
    assert_executable(conformance)?;
    installation.operator.validate_credentials()?;
    let context = &installation.target.kubernetes.context;
    let namespace = &installation.target.kubernetes.namespace;
    assert_showcase_gpu_workloads(context, namespace)?;
    let operator = OperatorClient {
        conformance,
        installation,
    };
    ensure_world_configured(&operator, &scenario).await?;
    let camera_products = wait_for_live_products(
        &operator,
        &scenario,
        PRIMARY_CAMERA_ID,
        Duration::from_secs(scenario.view.timeout_seconds),
    )
    .await?;
    println!(
        "UAV showcase is live: session={}, camera={}, shared-camera-products={camera_products}",
        scenario.session_id, PRIMARY_CAMERA_ID,
    );
    Ok(())
}

async fn wait_for_live_products(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    camera_id: &str,
    timeout: Duration,
) -> Result<usize> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(operator, scenario).await?;
        let camera = state
            .get("live_cameras")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("cameraId").and_then(Value::as_str) == Some(camera_id))
            });
        let products: Vec<LiveStreamProductState> = serde_json::from_value(
            state
                .get("stream_products")
                .cloned()
                .context("authoritative simulator omitted its shared camera products")?,
        )
        .context("authoritative simulator returned invalid shared camera products")?;
        let product_count = products.len();
        if json_string(&state, "/lifecycle").ok() == Some("running")
            && camera
                .and_then(|item| item.get("health"))
                .and_then(Value::as_str)
                == Some("healthy")
            && ready_camera_product_set_matches_contract(&products)
        {
            return Ok(product_count);
        }
        ensure!(
            json_string(&state, "/lifecycle").ok() != Some("failed")
                && tokio::time::Instant::now() < deadline,
            "authoritative UAV logical camera products did not become healthy within {timeout:?}: {state}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn monitor_flight(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    chrome_cdp_url: &str,
    public_base_url: &str,
    camera_id: &str,
    evidence_directory: &Path,
    capture_signals: VisualCaptureSignals,
) -> Result<FlightEvidence> {
    let timeout = Duration::from_secs(scenario.view.timeout_seconds);
    // The domain future owns deadlines for its prerequisites and flight commands.
    // Its sender closes on any failure, including after owned recovery finishes.
    capture_signals
        .takeoff_ready
        .await
        .context("flight ended before this run's takeoff was ready for capture")?;
    let takeoff = wait_for_checkpoint(
        operator,
        scenario,
        camera_id,
        FlightCheckpoint::Takeoff,
        None,
        Duration::from_secs(scenario.takeoff.state_timeout_seconds),
    )
    .await?;
    let recording_id = recording_id(&takeoff.0)?;
    let takeoff_capture = capture_console_live_app(
        chrome_cdp_url,
        public_base_url,
        camera_id,
        &evidence_directory.join("takeoff-follow-camera.png"),
        timeout,
    )
    .await?;
    let takeoff_evidence = checkpoint_evidence(
        FlightCheckpoint::Takeoff,
        &takeoff.0,
        takeoff.1,
        takeoff_capture,
    )?;
    let _ = capture_signals.takeoff_complete.send(());

    capture_signals
        .mission_ready
        .await
        .context("flight ended before this run completed its mission")?;
    let mission = wait_for_checkpoint(
        operator,
        scenario,
        camera_id,
        FlightCheckpoint::Mission,
        Some(
            takeoff
                .1
                .saturating_add(scenario.view.minimum_mission_sensor_frames),
        ),
        Duration::from_secs(scenario.mission.task_timeout_seconds),
    )
    .await?;
    let mission_capture = capture_console_live_app(
        chrome_cdp_url,
        public_base_url,
        camera_id,
        &evidence_directory.join("mission-follow-camera.png"),
        timeout,
    )
    .await?;
    let mission_evidence = checkpoint_evidence(
        FlightCheckpoint::Mission,
        &mission.0,
        mission.1,
        mission_capture,
    )?;
    let stream = capture_console_stream_app(
        chrome_cdp_url,
        public_base_url,
        &evidence_directory.join("mission-stream-live.png"),
        timeout,
    )
    .await?;
    let _ = capture_signals.stream_complete.send(());

    let recording = capture_console_recording(
        chrome_cdp_url,
        public_base_url,
        recording_id,
        &evidence_directory.join("recording-rerun.png"),
        timeout,
    )
    .await
    .context("capturing composed UAV Rerun evidence while its camera is airborne")?;
    let source_state = simulation_state(operator, scenario).await?;
    let source_timeline_seconds = source_state
        .get("simulation_time_s")
        .and_then(Value::as_f64)
        .context("UAV state omitted simulation_time_s")?;
    let viewer_timeline_seconds = recording.final_timeline_seconds();
    let source_to_viewer_seconds = source_timeline_seconds - viewer_timeline_seconds;
    ensure!(
        (-0.25..=1.0).contains(&source_to_viewer_seconds),
        "Rerun live playback is not close to its authoritative simulation timeline: \
         source={source_timeline_seconds:.3}s viewer={viewer_timeline_seconds:.3}s \
         lag={source_to_viewer_seconds:.3}s"
    );
    let recording_source_latency = RecordingSourceLatencyEvidence {
        sampled_at: Utc::now(),
        source_timeline_seconds,
        viewer_timeline_seconds,
        source_to_viewer_seconds,
    };
    let _ = capture_signals.moving_recording_complete.send(());

    let landing = wait_for_checkpoint(
        operator,
        scenario,
        camera_id,
        FlightCheckpoint::Landing,
        Some(mission.1.saturating_add(1)),
        Duration::from_secs(
            scenario
                .landing_timeout_seconds
                .saturating_add(scenario.stream.live_timeout_seconds)
                .saturating_add(scenario.stream.recording_replay.task_timeout_seconds)
                .saturating_add(scenario.reason.task_timeout_seconds),
        ),
    )
    .await?;
    let landing_capture = capture_console_live_app(
        chrome_cdp_url,
        public_base_url,
        camera_id,
        &evidence_directory.join("landing-follow-camera.png"),
        timeout,
    )
    .await?;
    let landing_evidence = checkpoint_evidence(
        FlightCheckpoint::Landing,
        &landing.0,
        landing.1,
        landing_capture,
    )?;

    Ok(FlightEvidence {
        recording_id,
        checkpoints: vec![takeoff_evidence, mission_evidence, landing_evidence],
        stream,
        recording,
        recording_source_latency,
    })
}

async fn wait_for_checkpoint(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    camera_id: &str,
    phase: FlightCheckpoint,
    minimum_sequence: Option<u64>,
    timeout: Duration,
) -> Result<(Value, u64)> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(operator, scenario).await?;
        let sensor_camera = state
            .get("cameras")
            .and_then(Value::as_array)
            .and_then(|items| {
                items.iter().find(|item| {
                    item.get("vehicle_id").and_then(Value::as_str)
                        == Some(scenario.vehicle_id.as_str())
                })
            })
            .context("UAV state omitted the selected native sensor camera")?;
        let logical_camera = state
            .get("live_cameras")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("cameraId").and_then(Value::as_str) == Some(camera_id))
            })
            .context("UAV state omitted the selected logical operator camera")?;
        let sequence = sensor_camera
            .get("frames_observed")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let flight_state = json_string(&state, "/vehicles/0/flight_state")?;
        let altitude = state
            .pointer("/vehicles/0/enu/up_m")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        let phase_ready = match phase {
            FlightCheckpoint::Takeoff => {
                flight_state == "flying" && altitude >= scenario.takeoff.minimum_reached_altitude_m
            }
            FlightCheckpoint::Mission => {
                flight_state == "flying"
                    && minimum_sequence.is_some_and(|minimum| sequence >= minimum)
            }
            FlightCheckpoint::Landing => {
                matches!(flight_state, "landed" | "standby")
                    && minimum_sequence.is_some_and(|minimum| sequence >= minimum)
            }
        };
        if sensor_camera.get("lifecycle").and_then(Value::as_str) == Some("ready")
            && logical_camera.get("health").and_then(Value::as_str) == Some("healthy")
            && phase_ready
        {
            return Ok((state, sequence));
        }
        ensure!(
            flight_state != "failed"
                && sensor_camera.get("lifecycle").and_then(Value::as_str) != Some("failed")
                && logical_camera.get("health").and_then(Value::as_str) != Some("failed")
                && tokio::time::Instant::now() < deadline,
            "{phase:?} checkpoint did not reach an advancing native sensor and healthy logical \
             operator camera within {timeout:?}: flight={state}, sensor={sensor_camera}, \
             logical={logical_camera}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn checkpoint_evidence(
    phase: FlightCheckpoint,
    state: &Value,
    native_sensor_frame_sequence: u64,
    console: ConsoleLiveCaptureEvidence,
) -> Result<FlightCheckpointEvidence> {
    Ok(FlightCheckpointEvidence {
        phase,
        captured_at: Utc::now(),
        flight_state: json_string(state, "/vehicles/0/flight_state")?.to_owned(),
        relative_altitude_m: state
            .pointer("/vehicles/0/enu/up_m")
            .and_then(Value::as_f64)
            .context("UAV checkpoint omitted relative altitude")?,
        native_sensor_frame_sequence,
        console,
    })
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

fn assert_showcase_gpu_workloads(context: &str, namespace: &str) -> Result<()> {
    run_checked(
        Path::new("kubectl"),
        ["--context", context, "cluster-info"].map(OsString::from),
        [],
    )
    .context("composed UAV showcase acceptance requires its Kubernetes cluster")?;
    for deployment in ["uav-sim", "view-mcp", "stream-mcp", "reason-mcp"] {
        run_checked(
            Path::new("kubectl"),
            [
                "--context".into(),
                context.into(),
                "-n".into(),
                namespace.into(),
                "rollout".into(),
                "status".into(),
                format!("deployment/{deployment}").into(),
                "--timeout=30m".into(),
            ],
            [],
        )
        .with_context(|| format!("{deployment} is not concurrently available"))?;
    }
    let gpu = run_checked(
        Path::new("kubectl"),
        [
            "--context".into(),
            context.into(),
            "-n".into(),
            namespace.into(),
            "exec".into(),
            "deployment/uav-sim".into(),
            "-c".into(),
            "isaac-sim".into(),
            "--".into(),
            "nvidia-smi".into(),
            "--query-gpu=name,uuid,driver_version".into(),
            "--format=csv,noheader".into(),
        ],
        [],
    )?;
    let gpu = parse_single_nvidia_smi_gpu(&gpu)?;
    let visible_devices = run_checked(
        Path::new("kubectl"),
        [
            "--context".into(),
            context.into(),
            "-n".into(),
            namespace.into(),
            "exec".into(),
            "deployment/uav-sim".into(),
            "-c".into(),
            "isaac-sim".into(),
            "--".into(),
            "printenv".into(),
            "NVIDIA_VISIBLE_DEVICES".into(),
        ],
        [],
    )?;
    let allocated_uuid = NvidiaGpuUuid::from_visible_devices(&visible_devices)?;
    ensure!(
        allocated_uuid == gpu.uuid,
        "uav-sim saw GPU {} but Kubernetes allocated {}",
        gpu.uuid.as_str(),
        allocated_uuid.as_str()
    );
    Ok(())
}

fn finish_phases<T>(
    domain_result: Result<()>,
    visual_result: Result<T>,
    evidence_directory: &Path,
    source_revision: &str,
    run_id: &str,
) -> Result<T> {
    let outcomes = PhaseOutcomes {
        schema: "veoveo.ai/uav-showcase-phase-outcomes/v1",
        completed_at: Utc::now(),
        source_revision,
        run_id,
        domain: PhaseOutcome::from_result(&domain_result),
        visual: PhaseOutcome::from_result(&visual_result),
    };
    let outcomes_path = evidence_directory.join("phase-outcomes.json");
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&outcomes_path)
        .with_context(|| format!("creating phase outcomes {}", outcomes_path.display()))?;
    serde_json::to_writer_pretty(file, &outcomes)?;
    if let Err(error) = &visual_result {
        eprintln!("UAV visual acceptance failed: {error:#}");
    }
    domain_result.context("composed UAV domain acceptance failed")?;
    visual_result.context("composed UAV visual acceptance failed")
}

#[cfg(test)]
mod outcome_tests {
    use super::*;

    #[test]
    fn domain_failure_preserves_visual_failure_and_refuses_overwrite() {
        struct Directory(std::path::PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let directory = Directory(
            std::env::temp_dir().join(format!("veoveo-flight-outcomes-{}", uuid::Uuid::now_v7())),
        );
        fs::create_dir(&directory.0).unwrap();
        let result = finish_phases::<()>(
            Err(anyhow::anyhow!("domain fixture failure")),
            Err(anyhow::anyhow!("visual fixture failure")),
            &directory.0,
            "source-fixture",
            "run-fixture",
        );
        assert!(result.is_err());
        let path = directory.0.join("phase-outcomes.json");
        let before = fs::read(&path).unwrap();
        let report: Value = serde_json::from_slice(&before).unwrap();
        assert_eq!(report["domain"], "failed");
        assert_eq!(report["visual"], "failed");
        assert!(finish_phases(Ok(()), Ok(()), &directory.0, "other", "other").is_err());
        assert_eq!(fs::read(path).unwrap(), before);
    }
}
