use chrono::Utc;
use serde::Serialize;

use super::browser::{
    ConsoleLiveCaptureEvidence, ConsoleRecordingCaptureEvidence, ConsoleStreamCaptureEvidence,
    capture_console_live_app, capture_console_recording, capture_console_stream_app,
    preflight_console_live_app,
};
use super::*;

const EVIDENCE_SCHEMA: &str = "veoveo.io/uav-showcase-acceptance-evidence/v3";
const PRIMARY_CAMERA_ID: &str = "follow";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FlightCheckpointEvidence {
    phase: FlightCheckpoint,
    captured_at: chrono::DateTime<Utc>,
    flight_state: String,
    relative_altitude_m: f64,
    encoded_frame_sequence: u64,
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
    session_id: String,
    camera_id: String,
    stream_product_id: String,
    camera_rig: &'static str,
    recording_id: String,
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
    recording_id: String,
    checkpoints: Vec<FlightCheckpointEvidence>,
    stream: ConsoleStreamCaptureEvidence,
    recording: ConsoleRecordingCaptureEvidence,
    recording_source_latency: RecordingSourceLatencyEvidence,
}

struct VisualCaptureSignals {
    stream_complete: tokio::sync::oneshot::Sender<()>,
    moving_recording_complete: tokio::sync::oneshot::Sender<()>,
}

pub(crate) async fn uav_showcase_verify(
    conformance: &Path,
    scenario_path: &Path,
    context: &str,
    namespace: &str,
    public_base_url: &str,
    chrome_cdp_url: &str,
    evidence_root: &Path,
) -> Result<()> {
    let scenario = UavAcceptanceScenario::load(scenario_path)?;
    assert_executable(conformance)?;
    let public_base_url = public_base_url.trim_end_matches('/');
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "composed UAV showcase acceptance requires public HTTPS"
    );
    assert_showcase_gpu_workloads(context, namespace)?;
    let operator = OperatorClient {
        conformance,
        base: public_base_url,
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
    let product_id = wait_for_live_product(
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

    let (stream_capture_complete, hold_live_stream) = tokio::sync::oneshot::channel();
    let (recording_capture_complete, hold_landing) = tokio::sync::oneshot::channel();
    let domain = uav_sim_verify_with_visual_hold(
        conformance,
        scenario_path,
        context,
        public_base_url,
        Some(UavVisualHolds {
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
            stream_complete: stream_capture_complete,
            moving_recording_complete: recording_capture_complete,
        },
    );
    let (domain_result, visual_result) = tokio::join!(domain, visual);
    domain_result.context("composed UAV domain acceptance failed")?;
    let flight = visual_result.context("composed UAV visual acceptance failed")?;

    let evidence = ShowcaseEvidence {
        schema: EVIDENCE_SCHEMA,
        completed_at: Utc::now(),
        source_revision,
        run_id,
        scenario_path: scenario_path.display().to_string(),
        session_id: scenario.session_id.clone(),
        camera_id: PRIMARY_CAMERA_ID.to_owned(),
        stream_product_id: product_id,
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
    context: &str,
    namespace: &str,
    public_base_url: &str,
) -> Result<()> {
    let scenario = UavAcceptanceScenario::load(scenario_path)?;
    assert_executable(conformance)?;
    let public_base_url = public_base_url.trim_end_matches('/');
    ensure!(
        url::Url::parse(public_base_url)?.scheme() == "https",
        "composed UAV showcase activation requires public HTTPS"
    );
    assert_showcase_gpu_workloads(context, namespace)?;
    let operator = OperatorClient {
        conformance,
        base: public_base_url,
    };
    ensure_world_configured(&operator, &scenario).await?;
    let product_id = wait_for_live_product(
        &operator,
        &scenario,
        PRIMARY_CAMERA_ID,
        Duration::from_secs(scenario.view.timeout_seconds),
    )
    .await?;
    println!(
        "UAV showcase is live: session={}, camera={}, product={product_id}",
        scenario.session_id, PRIMARY_CAMERA_ID
    );
    Ok(())
}

async fn wait_for_live_product(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    camera_id: &str,
    timeout: Duration,
) -> Result<String> {
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
        let product_id = camera
            .and_then(|camera| camera.get("streamProductId"))
            .and_then(Value::as_str);
        let product = product_id.and_then(|product_id| {
            state
                .get("stream_products")
                .and_then(Value::as_array)
                .and_then(|items| {
                    items.iter().find(|item| {
                        item.get("streamProductId").and_then(Value::as_str) == Some(product_id)
                    })
                })
        });
        if json_string(&state, "/lifecycle").ok() == Some("running")
            && camera
                .and_then(|item| item.get("health"))
                .and_then(Value::as_str)
                == Some("healthy")
            && product
                .and_then(|item| item.get("lifecycle"))
                .and_then(Value::as_str)
                == Some("ready")
            && product
                .and_then(|item| item.get("encodedFrames"))
                .and_then(Value::as_u64)
                .is_some_and(|frames| frames > 0)
        {
            return Ok(product_id.expect("checked above").to_owned());
        }
        ensure!(
            json_string(&state, "/lifecycle").ok() != Some("failed")
                && product
                    .and_then(|item| item.get("lifecycle"))
                    .and_then(Value::as_str)
                    != Some("failed")
                && tokio::time::Instant::now() < deadline,
            "authoritative UAV live product did not become healthy within {timeout:?}: {state}"
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

    let mission = wait_for_checkpoint(
        operator,
        scenario,
        camera_id,
        FlightCheckpoint::Mission,
        Some(
            takeoff
                .1
                .saturating_add(scenario.view.minimum_mission_pose_delta),
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
        &recording_id,
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
        let product = state
            .get("stream_products")
            .and_then(Value::as_array)
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item.get("cameraId").and_then(Value::as_str) == Some(camera_id))
            })
            .context("UAV state omitted the selected stream product")?;
        let sequence = product
            .get("encodedFrames")
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
        if product.get("lifecycle").and_then(Value::as_str) == Some("ready") && phase_ready {
            return Ok((state, sequence));
        }
        ensure!(
            flight_state != "failed"
                && product.get("lifecycle").and_then(Value::as_str) != Some("failed")
                && tokio::time::Instant::now() < deadline,
            "{phase:?} checkpoint did not reach an advancing authoritative camera within \
             {timeout:?}: flight={state}, product={product}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn checkpoint_evidence(
    phase: FlightCheckpoint,
    state: &Value,
    encoded_frame_sequence: u64,
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
        encoded_frame_sequence,
        console,
    })
}

fn recording_id(state: &Value) -> Result<String> {
    ensure!(
        json_string(state, "/recordings/0/catalog_lifecycle")? == "ready",
        "UAV recording has not reached the governed catalog: {}",
        state.pointer("/recordings/0").unwrap_or(&Value::Null)
    );
    let uri = json_string(state, "/recordings/0/recording_uri")?;
    let id = uri
        .strip_prefix("recording://recordings/")
        .context("UAV state returned a non-canonical recording URI")?;
    ensure!(
        uuid::Uuid::parse_str(id)?.get_version_num() == 7,
        "UAV recording identity must be UUIDv7"
    );
    Ok(id.to_owned())
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
