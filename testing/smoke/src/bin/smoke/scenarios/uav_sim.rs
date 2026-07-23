use std::process::Stdio;

use anyhow::ensure;
use serde::Deserialize;

use super::*;

const NAMESPACE: &str = "veoveo";
const GOOGLE_PHOTOREALISTIC_3D_TILES_ASSET_ID: u64 = 2_275_207;
const OPERATOR_PROFILE_SCOPES: &[&str] = &[
    "operator:use",
    "view:read",
    "view:write",
    "view:capture",
    "map:dataset:read",
    "time:read",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UavAcceptanceScenario {
    schema: String,
    session_id: String,
    frame_uri: String,
    vehicle_id: String,
    takeoff: TakeoffScenario,
    camera: CameraAcceptance,
    mission: MissionScenario,
    recording: RecordingAcceptance,
    perception: PerceptionScenario,
    reason: ReasonScenario,
    landing_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TakeoffScenario {
    relative_altitude_m: f64,
    minimum_reached_altitude_m: f64,
    state_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CameraAcceptance {
    detail_timeout_seconds: u64,
    operational: OperationalCameraAcceptance,
    aerial_detail: AerialCameraAcceptance,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OperationalCameraAcceptance {
    minimum_mean_luma: f64,
    minimum_non_black_fraction: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AerialCameraAcceptance {
    minimum_mean_luma: f64,
    minimum_dynamic_range: u64,
    minimum_non_black_fraction: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissionScenario {
    longitude_offset_degrees: f64,
    relative_altitude_m: f64,
    speed_mps: f64,
    hold_seconds: f64,
    task_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordingAcceptance {
    frozen_rows_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PerceptionScenario {
    range_lag_seconds: f64,
    range_duration_seconds: f64,
    maximum_frames: u64,
    task_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReasonScenario {
    prompt: String,
    maximum_frames: u64,
    task_timeout_seconds: u64,
}

struct OperatorClient<'a> {
    conformance: &'a Path,
    base: &'a str,
}

impl OperatorClient<'_> {
    async fn conformance(&self, operation: &[&str], timeout: Duration) -> Result<String> {
        let token = gateway_token(self.conformance, self.base).await?;
        gateway_conformance(self.conformance, self.base, &token, operation, timeout).await
    }

    async fn call_tool(&self, tool: &str, arguments: Value) -> Result<Value> {
        self.call_tool_with_timeout(tool, arguments, Duration::from_secs(120))
            .await
    }

    async fn call_tool_with_timeout(
        &self,
        tool: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value> {
        let arguments = serde_json::to_string(&arguments)?;
        let output = self
            .conformance(
                &["call", "--tool-name", tool, "--arguments", &arguments],
                timeout,
            )
            .await?;
        structured_output(&output).with_context(|| format!("tool {tool} returned invalid output"))
    }

    async fn task_tool(&self, tool: &str, arguments: Value, timeout: Duration) -> Result<Value> {
        let arguments = serde_json::to_string(&arguments)?;
        let output = self
            .conformance(
                &["task-call", "--tool-name", tool, "--arguments", &arguments],
                timeout,
            )
            .await?;
        structured_output(&output)
            .with_context(|| format!("task tool {tool} returned invalid output"))
    }
}

impl UavAcceptanceScenario {
    fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)
            .with_context(|| format!("reading UAV acceptance scenario {}", path.display()))?;
        let scenario: Self = serde_json::from_slice(&bytes)
            .with_context(|| format!("decoding UAV acceptance scenario {}", path.display()))?;
        scenario.validate()?;
        Ok(scenario)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "veoveo.uav-sim-acceptance/v4",
            "unsupported UAV acceptance scenario schema {:?}",
            self.schema
        );
        validate_identity("session_id", &self.session_id)?;
        validate_identity("vehicle_id", &self.vehicle_id)?;
        ensure!(
            self.frame_uri.starts_with("frames://frame/")
                && self.frame_uri.len() > "frames://frame/".len(),
            "frame_uri must use frames://frame/{{frame_id}}"
        );
        ensure!(
            self.takeoff.relative_altitude_m.is_finite()
                && (1.0..=10_000.0).contains(&self.takeoff.relative_altitude_m),
            "takeoff.relative_altitude_m must be between 1 and 10000"
        );
        ensure!(
            self.takeoff.minimum_reached_altitude_m.is_finite()
                && self.takeoff.minimum_reached_altitude_m > 0.0
                && self.takeoff.minimum_reached_altitude_m <= self.takeoff.relative_altitude_m,
            "takeoff.minimum_reached_altitude_m must be positive and no higher than takeoff"
        );
        ensure!(
            self.takeoff.state_timeout_seconds > 0
                && self.camera.detail_timeout_seconds > 0
                && self.mission.task_timeout_seconds > 0
                && self.recording.frozen_rows_timeout_seconds > 0
                && self.perception.task_timeout_seconds > 0
                && self.landing_timeout_seconds > 0,
            "scenario timeouts must be positive"
        );
        ensure!(
            self.camera.operational.minimum_mean_luma.is_finite()
                && (0.0..=255.0).contains(&self.camera.operational.minimum_mean_luma)
                && self
                    .camera
                    .operational
                    .minimum_non_black_fraction
                    .is_finite()
                && (0.0..=1.0).contains(&self.camera.operational.minimum_non_black_fraction)
                && self.camera.aerial_detail.minimum_mean_luma.is_finite()
                && (0.0..=255.0).contains(&self.camera.aerial_detail.minimum_mean_luma)
                && self.camera.aerial_detail.minimum_dynamic_range <= 255
                && self
                    .camera
                    .aerial_detail
                    .minimum_non_black_fraction
                    .is_finite()
                && (0.0..=1.0).contains(&self.camera.aerial_detail.minimum_non_black_fraction),
            "camera thresholds are outside RGB8 bounds"
        );
        ensure!(
            self.mission.longitude_offset_degrees.is_finite()
                && self.mission.longitude_offset_degrees.abs() <= 1.0
                && self.mission.longitude_offset_degrees != 0.0
                && self.mission.relative_altitude_m.is_finite()
                && (1.0..=10_000.0).contains(&self.mission.relative_altitude_m)
                && self.mission.speed_mps.is_finite()
                && (0.1..=100.0).contains(&self.mission.speed_mps)
                && self.mission.hold_seconds.is_finite()
                && (0.0..=3_600.0).contains(&self.mission.hold_seconds),
            "mission parameters are outside the accepted flight envelope"
        );
        ensure!(
            self.perception.range_lag_seconds.is_finite()
                && self.perception.range_lag_seconds >= 0.0
                && self.perception.range_duration_seconds.is_finite()
                && self.perception.range_duration_seconds > 0.0
                && (1..=10_000).contains(&self.perception.maximum_frames),
            "perception parameters must define a positive bounded capture"
        );
        ensure!(
            !self.reason.prompt.trim().is_empty()
                && self.reason.prompt.len() <= 8_192
                && (1..=1_024).contains(&self.reason.maximum_frames)
                && self.reason.task_timeout_seconds > 0,
            "reason parameters must define a bounded prompted observation"
        );
        Ok(())
    }
}

fn validate_identity(name: &str, value: &str) -> Result<()> {
    ensure!(
        (1..=128).contains(&value.len())
            && value
                .bytes()
                .all(|byte| { byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.') }),
        "{name} must contain 1-128 ASCII letters, digits, underscores, dashes, or dots"
    );
    Ok(())
}

pub(crate) async fn uav_sim_verify(
    conformance: &Path,
    scenario_path: &Path,
    context: &str,
    public_base_url: &str,
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
    let public = url::Url::parse(public_base_url).context("parsing public Bioma URL")?;
    ensure!(
        public.scheme() == "https",
        "UAV live acceptance requires public HTTPS"
    );

    run_checked(
        Path::new("kubectl"),
        ["--context", context, "cluster-info"].map(OsString::from),
        [],
    )
    .context("UAV live acceptance requires the Bioma Kubernetes cluster")?;
    assert_concurrent_gpu_workloads(context)?;

    let operator = OperatorClient {
        conformance,
        base: public_base_url,
    };
    let info = operator
        .conformance(&["info"], Duration::from_secs(60))
        .await?;
    for tool in [
        "uav-sim__get_simulation_state",
        "uav-sim__open_live_stream",
        "uav-sim__renew_live_stream",
        "uav-sim__close_live_stream",
        "uav-sim__execute_mission",
        "perception__analyze_recording",
        "reason__analyze_recording",
        "recording__query_recording",
    ] {
        contains(&info, tool)?;
    }

    let frame = operator
        .conformance(&["resource", &scenario.frame_uri], Duration::from_secs(60))
        .await?;
    for expected in ["13.6929", "-89.2182", "700.0", "enu"] {
        contains(&frame, expected)?;
    }
    contains(
        &frame,
        scenario
            .frame_uri
            .strip_prefix("frames://frame/")
            .context("validated frame URI omitted its frame identity")?,
    )?;

    let mut state = simulation_state(&operator, &scenario).await?;
    assert_world_ready(&state, &scenario)?;
    let live = operator
        .call_tool(
            "uav-sim__open_live_stream",
            serde_json::json!({"session_id": scenario.session_id}),
        )
        .await?;
    ensure!(
        json_string(&live, "/stream/hardware_encoder")? == "nvidia_nvenc"
            && json_string(&live, "/stream/codec")? == "h264"
            && json_string(&live, "/stream/source")? == "follow_camera"
            && !json_string(&live, "/access_token")?.is_empty(),
        "UAV live-stream lease is not NVIDIA accelerated: {live}"
    );
    ensure!(
        live.pointer("/endpoint/signaling_port")
            .and_then(Value::as_u64)
            .is_some_and(|port| port > 0)
            && live
                .pointer("/endpoint/media_port")
                .and_then(Value::as_u64)
                .is_some_and(|port| port > 0),
        "UAV live-stream endpoint is incomplete: {live}"
    );
    operator
        .call_tool(
            "uav-sim__close_live_stream",
            serde_json::json!({
                "session_id": scenario.session_id,
                "stream_id": json_string(&live, "/stream/stream_id")?,
            }),
        )
        .await?;
    let recording_uri = json_string(&state, "/recordings/0/recording_uri")?.to_owned();
    let recording_id = recording_uri
        .strip_prefix("recording://recordings/")
        .context("UAV state returned a non-canonical recording URI")?;
    ensure!(
        uuid::Uuid::parse_str(recording_id)?.get_version_num() == 7,
        "UAV recording identity must be UUIDv7"
    );
    let camera_entity = json_string(&state, "/recordings/0/camera_streams/0")?.to_owned();

    operator
        .call_tool(
            "uav-sim__arm_vehicle",
            serde_json::json!({
                "session_id": scenario.session_id,
                "vehicle_id": scenario.vehicle_id
            }),
        )
        .await?;
    wait_for_flight_state(&operator, &["armed"], Duration::from_secs(60), &scenario).await?;
    operator
        .call_tool(
            "uav-sim__takeoff_vehicle",
            serde_json::json!({
                "session_id": scenario.session_id,
                "vehicle_id": scenario.vehicle_id,
                "relative_altitude_m": scenario.takeoff.relative_altitude_m
            }),
        )
        .await?;
    state = wait_for_flight_state(
        &operator,
        &["flying"],
        Duration::from_secs(scenario.takeoff.state_timeout_seconds),
        &scenario,
    )
    .await?;
    ensure!(
        state
            .pointer("/vehicles/0/enu/up_m")
            .and_then(Value::as_f64)
            .is_some_and(|up_m| up_m >= scenario.takeoff.minimum_reached_altitude_m),
        "UAV did not reach the configured aerial-tiles acceptance altitude: {state}"
    );
    state = wait_for_aerial_camera_content(
        &operator,
        Duration::from_secs(scenario.camera.detail_timeout_seconds),
        &scenario,
    )
    .await?;

    let origin = state
        .get("georeference_origin")
        .and_then(Value::as_object)
        .context("UAV state omitted georeference_origin")?;
    let latitude = json_number(origin, "latitude_degrees")?;
    let longitude = json_number(origin, "longitude_degrees")?;
    let height = json_number(origin, "ellipsoid_height_m")?;
    let mission = serde_json::json!({
        "session_id": scenario.session_id,
        "mission_id": format!("acceptance-{}", uuid::Uuid::now_v7()),
        "frame_uri": scenario.frame_uri,
        "vehicles": [{
            "vehicle_id": scenario.vehicle_id,
            "waypoints": [{
                "position": {
                    "latitude_degrees": latitude,
                    "longitude_degrees": longitude
                        + scenario.mission.longitude_offset_degrees,
                    "ellipsoid_height_m": height
                        + scenario.mission.relative_altitude_m
                },
                "speed_mps": scenario.mission.speed_mps,
                "hold_seconds": scenario.mission.hold_seconds
            }]
        }]
    });
    let mission_output = operator
        .task_tool(
            "uav-sim__execute_mission",
            mission,
            Duration::from_secs(scenario.mission.task_timeout_seconds),
        )
        .await?;
    ensure!(
        json_string(&mission_output, "/lifecycle")? == "completed"
            && mission_output
                .get("completed_waypoints")
                .and_then(Value::as_u64)
                .is_some_and(|count| count >= 1),
        "UAV mission did not complete a waypoint: {mission_output}"
    );

    state = simulation_state(&operator, &scenario).await?;
    let simulation_time_s = state
        .get("simulation_time_s")
        .and_then(Value::as_f64)
        .context("UAV state omitted simulation_time_s")?;
    let range_end_s = simulation_time_s - scenario.perception.range_lag_seconds;
    let range_start_s = range_end_s - scenario.perception.range_duration_seconds;
    ensure!(
        range_start_s >= 0.0,
        "UAV recording has not accumulated enough stable aerial camera history"
    );
    let range_start = (range_start_s * 1_000_000_000.0) as i64;
    let range_end = (range_end_s * 1_000_000_000.0) as i64;

    wait_for_recording_camera_range(
        &operator,
        recording_id,
        &camera_entity,
        range_start,
        range_end,
        Duration::from_secs(scenario.recording.frozen_rows_timeout_seconds),
    )
    .await?;
    let perception = operator
        .task_tool(
            "perception__analyze_recording",
            serde_json::json!({
                "video": {
                    "recording_uri": recording_uri,
                    "entity_path": camera_entity,
                    "timeline": "simulation_time",
                    "range": {"start": range_start, "end": range_end}
                },
                "pipeline_id": "traffic-object-detection",
                "sampling": {
                    "mode": "maximum_frames",
                    "count": scenario.perception.maximum_frames
                },
                "include_source_clip": true
            }),
            Duration::from_secs(scenario.perception.task_timeout_seconds),
        )
        .await?;
    ensure!(
        perception
            .pointer("/summary/processed_frames")
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0),
        "Perception processed no Isaac camera frames: {perception}"
    );
    let governed_artifact_id =
        json_string(&perception, "/results_artifact/artifact_id")?.to_owned();
    ensure!(
        uuid::Uuid::parse_str(&governed_artifact_id)?.get_version_num() == 7,
        "Perception result artifact identity must be UUIDv7"
    );
    let grounding_uri = json_string(&perception, "/results_artifact/artifact_uri")?.to_owned();

    let reason = operator
        .task_tool(
            "reason__analyze_recording",
            serde_json::json!({
                "video": {
                    "recording_uri": recording_uri,
                    "entity_path": camera_entity,
                    "timeline": "simulation_time",
                    "range": {"start": range_start, "end": range_end}
                },
                "pipeline_id": "video-reasoning",
                "task": {
                    "kind": "describe_segment",
                    "prompt": scenario.reason.prompt
                },
                "sampling": {"max_frames": scenario.reason.maximum_frames},
                "grounding": {"results_artifact_uri": grounding_uri}
            }),
            Duration::from_secs(scenario.reason.task_timeout_seconds),
        )
        .await?;
    ensure!(
        reason
            .pointer("/summary/observed_frames")
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0),
        "Reason observed no Isaac camera frames: {reason}"
    );
    let reason_artifact_id = json_string(&reason, "/results_artifact/artifact_id")?.to_owned();
    ensure!(
        uuid::Uuid::parse_str(&reason_artifact_id)?.get_version_num() == 7,
        "Reason result artifact identity must be UUIDv7"
    );

    operator
        .call_tool(
            "uav-sim__land_vehicle",
            serde_json::json!({
                "session_id": scenario.session_id,
                "vehicle_id": scenario.vehicle_id
            }),
        )
        .await?;
    wait_for_flight_state(
        &operator,
        &["landed", "standby"],
        Duration::from_secs(scenario.landing_timeout_seconds),
        &scenario,
    )
    .await?;
    assert_concurrent_gpu_workloads(context)?;
    assert_governed_artifact_access(conformance, public_base_url, &governed_artifact_id).await?;

    println!(
        "UAV simulation acceptance ok: Google Photorealistic 3D Tiles were resident in Isaac, PX4 completed a mission, Recording Hub retained the world, Perception produced a governed artifact, Reason described the flight segment grounded in those detections, an authorized context member previewed it, an independent context was denied, and View remained available"
    );
    Ok(())
}

async fn assert_governed_artifact_access(
    conformance: &Path,
    base: &str,
    artifact_id: &str,
) -> Result<()> {
    let admin_token = gateway_token_for_context(
        conformance,
        base,
        "admin-service",
        "admin",
        &["operator:use", "admin:manage"],
        "operations",
    )
    .await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let snapshot: Value = client
        .get(format!("{base}/admin/admin/console/snapshot"))
        .bearer_auth(&admin_token)
        .send()
        .await
        .context("requesting the governed Console snapshot")?
        .error_for_status()
        .context("governed Console snapshot returned an error")?
        .json()
        .await
        .context("decoding the governed Console snapshot")?;
    let artifact = snapshot
        .get("artifacts")
        .and_then(Value::as_array)
        .and_then(|artifacts| {
            artifacts
                .iter()
                .find(|artifact| artifact.get("id").and_then(Value::as_str) == Some(artifact_id))
        })
        .with_context(|| format!("Console snapshot omitted governed artifact {artifact_id}"))?;
    ensure!(
        artifact
            .pointer("/provenance/workContext")
            .and_then(Value::as_str)
            == Some("operations")
            && artifact
                .pointer("/provenance/producer")
                .and_then(Value::as_str)
                .is_some_and(|producer| producer.ends_with("#operator-service"))
            && artifact
                .pointer("/provenance/invocationMode")
                .and_then(Value::as_str)
                == Some("automated")
            && artifact
                .pointer("/provenance/policyRevision")
                .and_then(Value::as_str)
                .is_some_and(|revision| !revision.is_empty())
            && artifact
                .pointer("/outputOwner/kind")
                .and_then(Value::as_str)
                == Some("group")
            && artifact.pointer("/outputOwner/id").and_then(Value::as_str) == Some("operations")
            && artifact
                .pointer("/effectiveAccess/read")
                .and_then(Value::as_bool)
                == Some(true),
        "governed artifact provenance or effective access is incomplete: {artifact}"
    );

    let download_url = format!("{base}/artifacts/operator/{artifact_id}/download");
    let authorized_token = gateway_token(conformance, base).await?;
    let preview = client
        .get(&download_url)
        .bearer_auth(&authorized_token)
        .send()
        .await
        .context("previewing the governed artifact as an authorized context member")?
        .error_for_status()
        .context("authorized governed artifact preview returned an error")?;
    let preview_media_type = preview
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let preview_bytes = preview.bytes().await?;
    let preview_media_type = preview_media_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim();
    ensure!(
        preview_media_type == "application/json"
            || (preview_media_type.starts_with("application/")
                && preview_media_type.ends_with("+json")),
        "authorized governed artifact preview returned media type `{preview_media_type}`"
    );
    let preview_json: Value = serde_json::from_slice(&preview_bytes)
        .context("authorized governed artifact preview contained invalid JSON")?;
    ensure!(
        preview_json.is_object(),
        "authorized governed artifact preview did not contain a JSON object"
    );

    let independent_token = gateway_token_for_context(
        conformance,
        base,
        "operator-service",
        "operator",
        OPERATOR_PROFILE_SCOPES,
        "independent-review",
    )
    .await?;
    let no_redirect = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let denied = no_redirect
        .get(download_url)
        .bearer_auth(independent_token)
        .send()
        .await
        .context("requesting the governed artifact from an independent Work Context")?;
    ensure!(
        denied.status() == reqwest::StatusCode::FORBIDDEN,
        "independent Work Context received {}, expected 403",
        denied.status()
    );
    Ok(())
}

async fn wait_for_recording_camera_range(
    operator: &OperatorClient<'_>,
    recording_id: &str,
    camera_entity: &str,
    range_start: i64,
    range_end: i64,
    timeout: Duration,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let recording = operator
            .call_tool(
                "recording__query_recording",
                serde_json::json!({
                    "recording_id": recording_id,
                    "entities": camera_entity,
                    "timeline": "simulation_time",
                    "range": {
                        "start": range_start,
                        "end": range_end
                    },
                    "max_rows": 1
                }),
            )
            .await?;
        if recording
            .get("rows_by_recording")
            .and_then(Value::as_object)
            .is_some_and(|rows| {
                rows.values()
                    .any(|count| count.as_u64().is_some_and(|count| count > 0))
            })
        {
            return Ok(recording);
        }
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "Recording Hub froze no UAV camera samples in range {range_start}..={range_end} within {timeout:?}: {recording}"
            );
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn assert_concurrent_gpu_workloads(context: &str) -> Result<()> {
    for deployment in ["uav-sim", "view-mcp", "perception-mcp", "reason-mcp"] {
        run_checked(
            Path::new("kubectl"),
            [
                "--context".into(),
                context.into(),
                "-n".into(),
                NAMESPACE.into(),
                "rollout".into(),
                "status".into(),
                format!("deployment/{deployment}").into(),
                "--timeout=30m".into(),
            ],
            [],
        )
        .with_context(|| format!("{deployment} is not concurrently available"))?;
    }
    Ok(())
}

fn assert_world_ready(state: &Value, scenario: &UavAcceptanceScenario) -> Result<()> {
    ensure!(
        matches!(
            json_string(state, "/lifecycle")?,
            "ready" | "running" | "paused"
        ),
        "UAV session is not ready: {state}"
    );
    ensure!(
        json_string(state, "/frame_uri")? == scenario.frame_uri,
        "UAV session uses the wrong Frames identity: {state}"
    );
    ensure!(
        json_string(state, "/tiles/source")? == "google_photorealistic_3d_tiles"
            && state.pointer("/tiles/ion_asset_id").and_then(Value::as_u64)
                == Some(GOOGLE_PHOTOREALISTIC_3D_TILES_ASSET_ID)
            && json_string(state, "/tiles/lifecycle")? == "ready"
            && state
                .pointer("/tiles/resident_tiles")
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0),
        "Google Photorealistic 3D Tiles are not resident inside Isaac: {state}"
    );
    ensure!(
        state
            .pointer("/vehicles/0/px4_connected")
            .and_then(Value::as_bool)
            == Some(true),
        "PX4 is not connected: {state}"
    );
    ensure!(
        json_string(state, "/cameras/0/lifecycle")? == "ready"
            && state
                .pointer("/cameras/0/frames_observed")
                .and_then(Value::as_u64)
                .is_some_and(|count| count >= 3)
            && state
                .pointer("/cameras/0/mean_luma")
                .and_then(Value::as_f64)
                .is_some_and(|value| value >= scenario.camera.operational.minimum_mean_luma)
            && state
                .pointer("/cameras/0/non_black_fraction")
                .and_then(Value::as_f64)
                .is_some_and(|value| {
                    value >= scenario.camera.operational.minimum_non_black_fraction
                }),
        "Isaac nadir camera is not operational: {state}"
    );
    ensure!(
        matches!(
            json_string(state, "/live_stream/lifecycle")?,
            "ready" | "live"
        ) && json_string(state, "/live_stream/source")? == "follow_camera"
            && json_string(state, "/live_stream/codec")? == "h264"
            && json_string(state, "/live_stream/hardware_encoder")? == "nvidia_nvenc"
            && state
                .pointer("/live_stream/width")
                .and_then(Value::as_u64)
                .is_some_and(|width| width >= 1280)
            && state
                .pointer("/live_stream/fps")
                .and_then(Value::as_u64)
                .is_some_and(|fps| fps >= 20),
        "NVIDIA follow-camera live streaming is not ready: {state}"
    );
    Ok(())
}

async fn simulation_state(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<Value> {
    const ATTEMPTS: usize = 3;
    let mut last_error = None;
    for attempt in 1..=ATTEMPTS {
        match operator
            .call_tool_with_timeout(
                "uav-sim__get_simulation_state",
                serde_json::json!({"session_id": scenario.session_id}),
                Duration::from_secs(30),
            )
            .await
        {
            Ok(state) => return Ok(state),
            Err(error) if attempt < ATTEMPTS => {
                eprintln!(
                    "UAV state read attempt {attempt}/{ATTEMPTS} failed; retrying: {error:#}"
                );
                last_error = Some(error);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.context("UAV state read exhausted its retry budget")?)
}

async fn wait_for_flight_state(
    operator: &OperatorClient<'_>,
    accepted: &[&str],
    timeout: Duration,
    scenario: &UavAcceptanceScenario,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(operator, scenario).await?;
        let flight_state = json_string(&state, "/vehicles/0/flight_state")?;
        if accepted.contains(&flight_state) {
            return Ok(state);
        }
        ensure!(
            flight_state != "failed",
            "PX4 entered the failed state: {state}"
        );
        if tokio::time::Instant::now() >= deadline {
            bail!("PX4 did not reach {accepted:?} within {timeout:?}; final state: {state}");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn wait_for_aerial_camera_content(
    operator: &OperatorClient<'_>,
    timeout: Duration,
    scenario: &UavAcceptanceScenario,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(operator, scenario).await?;
        let camera_has_detail = state
            .pointer("/cameras/0/mean_luma")
            .and_then(Value::as_f64)
            .is_some_and(|value| value >= scenario.camera.aerial_detail.minimum_mean_luma)
            && state
                .pointer("/cameras/0/dynamic_range")
                .and_then(Value::as_u64)
                .is_some_and(|value| value >= scenario.camera.aerial_detail.minimum_dynamic_range)
            && state
                .pointer("/cameras/0/non_black_fraction")
                .and_then(Value::as_f64)
                .is_some_and(|value| {
                    value >= scenario.camera.aerial_detail.minimum_non_black_fraction
                });
        if camera_has_detail {
            return Ok(state);
        }
        ensure!(
            json_string(&state, "/cameras/0/lifecycle")? != "failed",
            "Isaac nadir camera failed before aerial content became visible: {state}"
        );
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "Isaac nadir camera did not show detailed Google tiles within {timeout:?}; \
                 final state: {state}"
            );
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn gateway_token(conformance: &Path, base: &str) -> Result<String> {
    gateway_token_for_context(
        conformance,
        base,
        "operator-service",
        "operator",
        OPERATOR_PROFILE_SCOPES,
        "operations",
    )
    .await
}

async fn gateway_token_for_context(
    conformance: &Path,
    base: &str,
    client_id: &str,
    profile: &str,
    scopes: &[&str],
    work_context: &str,
) -> Result<String> {
    let token_url = format!("{base}/oauth/token");
    let resource = format!("{base}/mcp/{profile}");
    let mut command = tokio::process::Command::new(conformance);
    command
        .args([
            "gateway-token-exchange",
            "--token-url",
            &token_url,
            "--client-id",
            client_id,
            "--audience",
            &token_url,
            "--resource",
            &resource,
            "--work-context",
            work_context,
        ])
        .args(
            scopes
                .iter()
                .flat_map(|scope| ["--scope", *scope])
                .collect::<Vec<_>>(),
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

fn json_number(object: &serde_json::Map<String, Value>, key: &str) -> Result<f64> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .with_context(|| format!("georeference_origin omitted numeric {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_scenario() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../showcase/uav-sim/scenarios/bioma-aerial.json")
    }

    #[test]
    fn canonical_mission_is_runtime_loaded_and_validated() {
        let scenario = UavAcceptanceScenario::load(&canonical_scenario()).unwrap();
        assert_eq!(scenario.schema, "veoveo.uav-sim-acceptance/v4");
        assert_eq!(scenario.session_id, "bioma-uav");
        assert_eq!(scenario.takeoff.relative_altitude_m, 300.0);
        assert_eq!(scenario.mission.speed_mps, 3.0);
        assert_eq!(scenario.recording.frozen_rows_timeout_seconds, 1_200);
        assert_eq!(scenario.camera.aerial_detail.minimum_dynamic_range, 8);
        assert_eq!(scenario.perception.range_lag_seconds, 10.0);
        assert!(!scenario.reason.prompt.is_empty());
        assert_eq!(scenario.reason.maximum_frames, 8);
    }

    #[test]
    fn mission_file_is_outside_the_isaac_image_build_context() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let scenario = canonical_scenario().canonicalize().unwrap();
        let runtime_context = root
            .join("showcase/uav-sim/runtime")
            .canonicalize()
            .unwrap();
        assert!(!scenario.starts_with(runtime_context));
    }
}
