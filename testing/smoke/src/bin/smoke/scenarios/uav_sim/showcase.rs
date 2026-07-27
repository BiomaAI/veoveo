use chrono::{SecondsFormat, Utc};
use serde::Serialize;

use super::*;
use crate::scenarios::simulation_view::browser::{
    ConsoleLiveCaptureEvidence, ConsoleRecordingCaptureEvidence, capture_console_live_app,
    capture_console_recording,
};

const EVIDENCE_SCHEMA: &str = "veoveo.io/uav-showcase-acceptance-evidence/v1";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FlightCheckpointEvidence {
    phase: FlightCheckpoint,
    captured_at: chrono::DateTime<Utc>,
    flight_state: String,
    relative_altitude_m: f64,
    pose_sequence: u64,
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
    scene_digest: String,
    pose_producer_id: String,
    pose_producer_spiffe_id: String,
    camera_id: String,
    camera_rig: &'static str,
    recording_id: String,
    checkpoints: Vec<FlightCheckpointEvidence>,
    recording: ConsoleRecordingCaptureEvidence,
}

#[derive(Debug)]
struct ViewResources {
    session_id: String,
    session_revision: u64,
    camera_id: String,
    camera_revision: u64,
}

struct FlightEvidence {
    recording_id: String,
    checkpoints: Vec<FlightCheckpointEvidence>,
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
    if conformance == Path::new("target/debug/conformance") {
        run_checked(
            Path::new("cargo"),
            [
                "build".into(),
                "-p".into(),
                "veoveo-mcp-conformance".into(),
                "--bin".into(),
                "conformance".into(),
            ],
            [],
        )?;
    }
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
        "uav-sim__prepare_view_scene",
        "simulation-view__create_session",
        "simulation-view__bind_scene",
        "simulation-view__authorize_pose_producer",
        "simulation-view__create_camera",
        "simulation-view__close_camera",
        "simulation-view__close_session",
    ] {
        contains(&info, tool)?;
    }

    ensure_world_configured(&operator, &scenario).await?;
    close_existing_view(&operator, &scenario.session_id).await?;
    let prepared = wait_for_prepared_scene(
        &operator,
        &scenario,
        Duration::from_secs(scenario.world_ready_timeout_seconds),
    )
    .await?;
    let scene = prepared
        .get("scene")
        .cloned()
        .context("UAV scene preparation omitted scene")?;
    let scene_digest = json_string(&prepared, "/scene/digest")?.to_owned();
    let epoch_id = json_string(&prepared, "/scene/body/epochId")?.to_owned();
    let producer_id = json_string(&prepared, "/producer_id")?.to_owned();
    let producer_spiffe_id = json_string(&prepared, "/producer_spiffe_id")?.to_owned();
    ensure!(
        json_string(&prepared, "/scene/body/sessionId")? == scenario.session_id,
        "UAV scene declaration changed its authoritative session identity: {prepared}"
    );

    let mut resources = create_view(
        &operator,
        &scenario,
        scene,
        &epoch_id,
        &producer_id,
        &producer_spiffe_id,
    )
    .await?;
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

    let domain = uav_sim_verify(conformance, scenario_path, context, public_base_url);
    let visual = monitor_flight(
        &operator,
        &scenario,
        chrome_cdp_url,
        public_base_url,
        &resources.camera_id,
        &evidence_directory,
    );
    // Do not cancel domain acceptance when visual acceptance fails after
    // takeoff. The domain future owns postflight landing recovery and must run
    // to completion on every composed path.
    let acceptance = tokio::join!(domain, visual);
    let flight = match acceptance {
        (Ok(()), Ok(flight)) => flight,
        (Err(domain_error), Ok(_)) => {
            let cleanup = cleanup_view(&operator, &mut resources).await;
            cleanup?;
            return Err(domain_error.context("composed UAV domain acceptance failed"));
        }
        (Ok(()), Err(visual_error)) => {
            let cleanup = cleanup_view(&operator, &mut resources).await;
            cleanup?;
            return Err(visual_error.context("composed UAV visual acceptance failed"));
        }
        (Err(domain_error), Err(visual_error)) => {
            let cleanup = cleanup_view(&operator, &mut resources).await;
            cleanup?;
            bail!(
                "composed UAV domain acceptance failed: {domain_error:#}; visual acceptance also \
                 failed: {visual_error:#}"
            );
        }
    };

    let recording = capture_console_recording(
        chrome_cdp_url,
        public_base_url,
        &flight.recording_id,
        &evidence_directory.join("recording-rerun.png"),
        Duration::from_secs(scenario.view.timeout_seconds),
    )
    .await;
    let cleanup = cleanup_view(&operator, &mut resources).await;
    let recording = recording?;
    cleanup?;

    let evidence = ShowcaseEvidence {
        schema: EVIDENCE_SCHEMA,
        completed_at: Utc::now(),
        source_revision,
        run_id,
        scenario_path: scenario_path.display().to_string(),
        session_id: scenario.session_id.clone(),
        scene_digest,
        pose_producer_id: producer_id,
        pose_producer_spiffe_id: producer_spiffe_id,
        camera_id: resources.camera_id,
        camera_rig: "follow_entity",
        recording_id: flight.recording_id,
        checkpoints: flight.checkpoints,
        recording,
    };
    let manifest_path = evidence_directory.join("evidence.json");
    fs::write(&manifest_path, serde_json::to_vec_pretty(&evidence)?)
        .with_context(|| format!("writing acceptance evidence {}", manifest_path.display()))?;
    println!(
        "UAV showcase acceptance ok: the UAV-owned governed scene drove an independent Simulation \
         View follow camera through takeoff, mission, and landing; the authenticated Console \
         displayed advancing NVIDIA NVENC H.264 at every checkpoint and opened the governed Rerun \
         recording. Evidence: {}",
        manifest_path.display()
    );
    Ok(())
}

async fn close_existing_view(operator: &OperatorClient<'_>, session_id: &str) -> Result<()> {
    let state = match operator
        .call_tool(
            "simulation-view__get_session_state",
            serde_json::json!({"sessionId": session_id}),
        )
        .await
    {
        Ok(state) => state,
        Err(_) => return Ok(()),
    };
    if json_string(&state, "/lifecycle").ok() == Some("closed") {
        return Ok(());
    }
    let closed = operator
        .call_tool(
            "simulation-view__close_session",
            serde_json::json!({
                "sessionId": session_id,
                "expectedRevision": json_u64(&state, "/revision")?,
            }),
        )
        .await?;
    ensure!(
        closed.get("closed").and_then(Value::as_bool) == Some(true),
        "stale Simulation View session did not close: {closed}"
    );
    Ok(())
}

async fn wait_for_prepared_scene(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    timeout: Duration,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match operator
            .call_tool(
                "uav-sim__prepare_view_scene",
                serde_json::json!({"session_id": scenario.session_id}),
            )
            .await
        {
            Ok(prepared) => return Ok(prepared),
            Err(error) if tokio::time::Instant::now() >= deadline => {
                return Err(error.context(format!(
                    "UAV did not prepare its governed Simulation View scene within {timeout:?}"
                )));
            }
            Err(_) => {}
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn create_view(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    scene: Value,
    epoch_id: &str,
    producer_id: &str,
    producer_spiffe_id: &str,
) -> Result<ViewResources> {
    let created = operator
        .call_tool(
            "simulation-view__create_session",
            serde_json::json!({
                "sessionId": scenario.session_id,
                "epochId": epoch_id,
            }),
        )
        .await?;
    let mut session_revision = json_u64(&created, "/revision")?;
    let bound = operator
        .call_tool(
            "simulation-view__bind_scene",
            serde_json::json!({
                "sessionId": scenario.session_id,
                "expectedRevision": session_revision,
                "scene": scene,
            }),
        )
        .await?;
    session_revision = json_u64(&bound, "/revision")?;
    ensure!(
        json_string(&bound, "/lifecycle")? == "scene_bound",
        "Simulation View did not bind the UAV-owned scene: {bound}"
    );
    let authorized = operator
        .call_tool(
            "simulation-view__authorize_pose_producer",
            serde_json::json!({
                "sessionId": scenario.session_id,
                "expectedRevision": session_revision,
                "producerId": producer_id,
                "spiffeId": producer_spiffe_id,
                "expiresAt": (Utc::now() + chrono::Duration::minutes(30))
                    .to_rfc3339_opts(SecondsFormat::Millis, true),
            }),
        )
        .await?;
    ensure!(
        json_string(&authorized, "/producerId")? == producer_id
            && json_string(&authorized, "/spiffeId")? == producer_spiffe_id,
        "Simulation View authorized a different UAV pose producer: {authorized}"
    );
    session_revision += 1;

    let camera = &scenario.view.camera;
    let admission = operator
        .call_tool(
            "simulation-view__create_camera",
            serde_json::json!({
                "sessionId": scenario.session_id,
                "definition": {
                    "rig": {
                        "kind": "follow_entity",
                        "targetEntity": scenario.vehicle_id,
                        "offsetFluM": {
                            "x": camera.offset_flu_m.x,
                            "y": camera.offset_flu_m.y,
                            "z": camera.offset_flu_m.z,
                        },
                        "smoothingSeconds": camera.smoothing_seconds,
                    },
                    "widthPx": camera.width_px,
                    "heightPx": camera.height_px,
                    "frameRateMillihertz": camera.frame_rate_millihertz,
                    "verticalFovDegrees": camera.vertical_fov_degrees,
                    "nearClipM": camera.near_clip_m,
                    "farClipM": camera.far_clip_m,
                    "streamPolicy": "on_demand",
                    "recordingPolicy": "disabled",
                }
            }),
        )
        .await?;
    ensure!(
        json_string(&admission, "/status")? == "admitted",
        "UAV follow camera was not admitted at its requested quality: {admission}"
    );
    Ok(ViewResources {
        session_id: scenario.session_id.clone(),
        session_revision,
        camera_id: json_string(&admission, "/camera/cameraId")?.to_owned(),
        camera_revision: json_u64(&admission, "/camera/revision")?,
    })
}

async fn monitor_flight(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    chrome_cdp_url: &str,
    public_base_url: &str,
    camera_id: &str,
    evidence_directory: &Path,
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

    let landing = wait_for_checkpoint(
        operator,
        scenario,
        camera_id,
        FlightCheckpoint::Landing,
        Some(mission.1.saturating_add(1)),
        Duration::from_secs(
            scenario
                .landing_timeout_seconds
                .saturating_add(scenario.perception.task_timeout_seconds)
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
    let camera_uri = format!(
        "simulation-view://session/{}/camera/{camera_id}",
        scenario.session_id
    );
    loop {
        let state = simulation_state(operator, scenario).await?;
        let camera = read_json_resource(operator, &camera_uri).await?;
        let flight_state = json_string(&state, "/vehicles/0/flight_state")?;
        let altitude = state
            .pointer("/vehicles/0/enu/up_m")
            .and_then(Value::as_f64)
            .unwrap_or_default();
        let sequence = camera
            .get("lastPoseSequence")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        let camera_ready = json_string(&camera, "/health").ok() == Some("healthy")
            && camera.get("lastFrameAt").and_then(Value::as_str).is_some();
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
        if camera_ready && phase_ready {
            return Ok((state, sequence));
        }
        ensure!(
            flight_state != "failed"
                && json_string(&camera, "/health").ok() != Some("failed")
                && tokio::time::Instant::now() < deadline,
            "{phase:?} checkpoint did not reach an advancing healthy follow camera within \
             {timeout:?}: flight={state}, camera={camera}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn checkpoint_evidence(
    phase: FlightCheckpoint,
    state: &Value,
    pose_sequence: u64,
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
        pose_sequence,
        console,
    })
}

fn recording_id(state: &Value) -> Result<String> {
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

async fn read_json_resource(operator: &OperatorClient<'_>, uri: &str) -> Result<Value> {
    serde_json::from_str(
        &operator
            .conformance(&["resource", uri], Duration::from_secs(60))
            .await?,
    )
    .with_context(|| format!("decoding MCP resource {uri}"))
}

async fn cleanup_view(operator: &OperatorClient<'_>, resources: &mut ViewResources) -> Result<()> {
    let mut first_error = None;
    if !resources.camera_id.is_empty()
        && let Err(error) = operator
            .call_tool(
                "simulation-view__close_camera",
                serde_json::json!({
                    "sessionId": resources.session_id,
                    "cameraId": resources.camera_id,
                    "expectedRevision": resources.camera_revision,
                }),
            )
            .await
    {
        first_error = Some(error);
    }
    if let Err(error) = operator
        .call_tool(
            "simulation-view__close_session",
            serde_json::json!({
                "sessionId": resources.session_id,
                "expectedRevision": resources.session_revision,
            }),
        )
        .await
    {
        first_error.get_or_insert(error);
    }
    if let Some(error) = first_error {
        return Err(error.context("cleaning composed Simulation View state"));
    }
    Ok(())
}

fn assert_showcase_gpu_workloads(context: &str, namespace: &str) -> Result<()> {
    run_checked(
        Path::new("kubectl"),
        ["--context", context, "cluster-info"].map(OsString::from),
        [],
    )
    .context("composed UAV showcase acceptance requires its Kubernetes cluster")?;
    for deployment in [
        "uav-sim",
        "simulation-view-renderer",
        "simulation-view-mcp",
        "view-mcp",
        "perception-mcp",
        "reason-mcp",
    ] {
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
    for (deployment, container) in [
        ("uav-sim", "isaac-sim"),
        ("simulation-view-renderer", "simulation-view-isaac"),
    ] {
        let gpu = run_checked(
            Path::new("kubectl"),
            [
                "--context".into(),
                context.into(),
                "-n".into(),
                namespace.into(),
                "exec".into(),
                format!("deployment/{deployment}").into(),
                "-c".into(),
                container.into(),
                "--".into(),
                "nvidia-smi".into(),
                "--query-gpu=name,uuid,driver_version".into(),
                "--format=csv,noheader".into(),
            ],
            [],
        )?;
        let fingerprint = gpu.to_ascii_lowercase();
        ensure!(
            fingerprint.contains("nvidia")
                && !fingerprint.contains("software")
                && !fingerprint.contains("llvmpipe"),
            "{deployment} did not expose an NVIDIA hardware GPU: {gpu}"
        );
    }
    Ok(())
}

fn json_u64(value: &Value, pointer: &str) -> Result<u64> {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .with_context(|| format!("JSON output omitted integer {pointer}: {value}"))
}
