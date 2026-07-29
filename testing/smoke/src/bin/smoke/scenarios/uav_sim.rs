use std::process::Stdio;

use anyhow::ensure;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;
use veoveo_mcp_contract::{
    FrameBasis, FrameId, FrameNode, FrameParentTransform, FrameWorldId, FrameWorldRevision,
    FrameWorldTree, Wgs84Position,
};
use veoveo_stream_mcp::contract::{
    LiveSessionLifecycle, LiveSessionView, StartLiveSessionOutput, StopLiveSessionOutput,
};

use super::*;

#[path = "uav_sim/showcase.rs"]
mod showcase;

pub(crate) use showcase::uav_showcase_verify;

const NAMESPACE: &str = "veoveo";
const GOOGLE_PHOTOREALISTIC_3D_TILES_ASSET_ID: u64 = 2_275_207;
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UavAcceptanceScenario {
    schema: String,
    session_id: String,
    world: FrameWorldScenario,
    vehicle_id: String,
    world_ready_timeout_seconds: u64,
    takeoff: TakeoffScenario,
    camera: CameraAcceptance,
    mission: MissionScenario,
    recording: RecordingAcceptance,
    stream: StreamScenario,
    reason: ReasonScenario,
    view: ViewAcceptance,
    landing_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameWorldScenario {
    world_id: FrameWorldId,
    display_name: String,
    description: String,
    simulation_frame_id: FrameId,
    tree: FrameWorldTree,
}

impl FrameWorldScenario {
    fn origin(&self) -> Result<&Wgs84Position> {
        self.tree
            .frames
            .iter()
            .find_map(|frame| match &frame.parent_transform {
                Some(FrameParentTransform::GeodeticTangent { origin }) => Some(origin),
                _ => None,
            })
            .context("world tree omitted a geodetic tangent anchor")
    }
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
    live_rows_timeout_seconds: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StreamScenario {
    live_pipeline_id: String,
    minimum_live_frames: u64,
    maximum_result_age_ms: u64,
    live_timeout_seconds: u64,
    recording_replay: RecordingReplayAcceptance,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordingReplayAcceptance {
    range_lag_seconds: f64,
    freshness_probe_duration_seconds: f64,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewAcceptance {
    timeout_seconds: u64,
    minimum_mission_pose_delta: u64,
    camera: ViewCameraAcceptance,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewCameraAcceptance {
    width_px: u32,
    height_px: u32,
    frame_rate_millihertz: u32,
    vertical_fov_degrees: f64,
    near_clip_m: f64,
    far_clip_m: f64,
    offset_flu_m: ViewOffset,
    smoothing_seconds: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewOffset {
    x: f64,
    y: f64,
    z: f64,
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
        let timeout_seconds = timeout.as_secs().max(1).to_string();
        let output = self
            .conformance(
                &[
                    "task-call",
                    "--tool-name",
                    tool,
                    "--arguments",
                    &arguments,
                    "--timeout-seconds",
                    &timeout_seconds,
                ],
                timeout
                    .checked_add(Duration::from_secs(30))
                    .context("task timeout overflowed the subprocess margin")?,
            )
            .await?;
        structured_output(&output)
            .with_context(|| format!("task tool {tool} returned invalid output"))
    }

    async fn resource_text(&self, uri: &str, timeout: Duration) -> Result<String> {
        self.conformance(&["resource", uri], timeout).await
    }

    async fn resource(&self, uri: &str, timeout: Duration) -> Result<Value> {
        let output = self.resource_text(uri, timeout).await?;
        serde_json::from_str(&output)
            .with_context(|| format!("resource {uri} returned invalid JSON"))
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
            self.schema == "veoveo.uav-sim-acceptance/v9",
            "unsupported UAV acceptance scenario schema {:?}",
            self.schema
        );
        validate_identity("session_id", &self.session_id)?;
        validate_identity("vehicle_id", &self.vehicle_id)?;
        ensure!(
            !self.world.display_name.trim().is_empty(),
            "world display name must not be blank"
        );
        ensure!(
            !self.world.description.trim().is_empty(),
            "world description must not be blank"
        );
        ensure!(
            self.world
                .tree
                .frames
                .iter()
                .any(|frame| frame.frame_id == self.world.simulation_frame_id),
            "simulation_frame_id must identify a frame in the world tree"
        );
        ensure!(
            self.world
                .tree
                .frames
                .iter()
                .filter(|frame| {
                    frame.parent_frame_id.is_none() && frame.basis == FrameBasis::EcefWgs84
                })
                .count()
                == 1,
            "world tree must contain one ECEF root"
        );
        let origin = self.world.origin()?;
        ensure!(
            origin.latitude_degrees.is_finite()
                && (-90.0..=90.0).contains(&origin.latitude_degrees)
                && origin.longitude_degrees.is_finite()
                && (-180.0..=180.0).contains(&origin.longitude_degrees)
                && origin.ellipsoid_height_m.is_finite()
                && (-100_000.0..=100_000.0).contains(&origin.ellipsoid_height_m),
            "origin must contain bounded WGS84 coordinates"
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
            self.world_ready_timeout_seconds > 0
                && self.takeoff.state_timeout_seconds > 0
                && self.camera.detail_timeout_seconds > 0
                && self.mission.task_timeout_seconds > 0
                && self.recording.live_rows_timeout_seconds > 0
                && self.stream.live_timeout_seconds > 0
                && self.stream.recording_replay.task_timeout_seconds > 0
                && self.view.timeout_seconds > 0
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
            !self.stream.live_pipeline_id.trim().is_empty()
                && self.stream.minimum_live_frames > 0
                && self.stream.maximum_result_age_ms > 0
                && self.stream.live_timeout_seconds > 0,
            "Stream live acceptance must require a pipeline, frames, freshness, and timeout"
        );
        let replay = &self.stream.recording_replay;
        ensure!(
            replay.range_lag_seconds.is_finite()
                && replay.range_lag_seconds >= 0.0
                && replay.range_lag_seconds <= 2.0
                && replay.freshness_probe_duration_seconds.is_finite()
                && replay.freshness_probe_duration_seconds > 0.0
                && replay.freshness_probe_duration_seconds <= replay.range_duration_seconds
                && replay.range_duration_seconds.is_finite()
                && replay.range_duration_seconds > 0.0
                && (1..=10_000).contains(&replay.maximum_frames)
                && replay.task_timeout_seconds > 0,
            "Stream replay must probe a positive range no more than two seconds behind the live edge"
        );
        ensure!(
            !self.reason.prompt.trim().is_empty()
                && self.reason.prompt.len() <= 8_192
                && (1..=1_024).contains(&self.reason.maximum_frames)
                && self.reason.task_timeout_seconds > 0,
            "reason parameters must define a bounded prompted observation"
        );
        let view = &self.view.camera;
        ensure!(
            (64..=7680).contains(&view.width_px)
                && (64..=4320).contains(&view.height_px)
                && (1_000..=240_000).contains(&view.frame_rate_millihertz)
                && view.vertical_fov_degrees.is_finite()
                && (1.0..179.0).contains(&view.vertical_fov_degrees)
                && view.near_clip_m.is_finite()
                && view.near_clip_m > 0.0
                && view.far_clip_m.is_finite()
                && view.far_clip_m > view.near_clip_m
                && [
                    view.offset_flu_m.x,
                    view.offset_flu_m.y,
                    view.offset_flu_m.z
                ]
                .into_iter()
                .all(f64::is_finite)
                && view.smoothing_seconds.is_finite()
                && (0.0..=60.0).contains(&view.smoothing_seconds)
                && self.view.minimum_mission_pose_delta > 0,
            "view parameters must define one bounded follow camera and advancing mission checkpoint"
        );
        Ok(())
    }
}

struct WorldBinding {
    revision_uri: String,
    simulation_frame_uri: String,
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
    uav_sim_verify_with_visual_hold(conformance, scenario_path, context, public_base_url, None)
        .await
}

async fn uav_sim_verify_with_visual_hold(
    conformance: &Path,
    scenario_path: &Path,
    context: &str,
    public_base_url: &str,
    visual_stream_capture: Option<oneshot::Receiver<()>>,
) -> Result<()> {
    let scenario = UavAcceptanceScenario::load(scenario_path)?;
    assert_executable(conformance)?;
    let public_base_url = public_base_url.trim_end_matches('/');
    let public = url::Url::parse(public_base_url).context("parsing public installation URL")?;
    ensure!(
        public.scheme() == "https",
        "UAV live acceptance requires public HTTPS"
    );

    run_checked(
        Path::new("kubectl"),
        ["--context", context, "cluster-info"].map(OsString::from),
        [],
    )
    .context("UAV live acceptance requires its Kubernetes cluster")?;
    assert_concurrent_gpu_workloads(context)?;

    let operator = OperatorClient {
        conformance,
        base: public_base_url,
    };
    let info = operator
        .conformance(&["info"], Duration::from_secs(60))
        .await?;
    for tool in [
        "frames__create_world",
        "frames__publish_world",
        "uav-sim__configure_world",
        "uav-sim__get_simulation_state",
        "uav-sim__execute_mission",
        "stream__start_live_session",
        "stream__stop_live_session",
        "stream__run_recording",
        "reason__analyze_recording",
        "recording__query_recording",
    ] {
        contains(&info, tool)?;
    }

    let binding = ensure_world_configured(&operator, &scenario).await?;
    let revision_uri = binding.revision_uri;
    let simulation_frame_uri = binding.simulation_frame_uri;

    let mut state = wait_for_world_ready(
        &operator,
        &scenario,
        &revision_uri,
        &simulation_frame_uri,
        Duration::from_secs(scenario.world_ready_timeout_seconds),
    )
    .await?;
    assert_georeference_origin(&state, &scenario)?;
    ensure!(
        json_string(&state, "/cameras/0/codec")? == "h264"
            && json_string(&state, "/cameras/0/encoder")? == "nvidia_nvenc",
        "UAV camera did not fail closed on the canonical NVIDIA NVENC H.264 path: {state}"
    );
    let recording_uri = json_string(&state, "/recordings/0/recording_uri")?.to_owned();
    let recording_id = recording_uri
        .strip_prefix("recording://recordings/")
        .context("UAV state returned a non-canonical recording URI")?;
    ensure!(
        uuid::Uuid::parse_str(recording_id)?.get_version_num() == 7,
        "UAV recording identity must be UUIDv7"
    );
    let camera_entity = json_string(&state, "/recordings/0/camera_streams/0")?.to_owned();

    let stream_app = operator
        .resource_text("ui://stream/live.html", Duration::from_secs(60))
        .await
        .context("reading the Stream MCP App through the gateway")?;
    ensure!(
        stream_app.contains("VideoDecoder")
            && stream_app.contains("/preview")
            && stream_app.contains("software H.264 decode")
            && stream_app.contains("hardware H.264 decode"),
        "Stream MCP App does not expose video decoding, preview, and decode-path status"
    );
    recover_live_stream_pipeline(&operator, &scenario.stream.live_pipeline_id).await?;
    let live: StartLiveSessionOutput = serde_json::from_value(
        operator
            .call_tool(
                "stream__start_live_session",
                serde_json::json!({
                    "pipeline_id": scenario.stream.live_pipeline_id
                }),
            )
            .await
            .context("starting the recording-independent live Stream session")?,
    )
    .context("decoding the typed live Stream session")?;
    ensure!(
        live.results_uri == format!("stream://session/{}/results", live.session_id)
            && live.preview_uri == format!("stream://session/{}/preview", live.session_id),
        "Stream returned inconsistent live-session resources: {live:?}"
    );
    let live_session_id = live.session_id;
    let live_preview_uri = live.preview_uri;
    let mut live_session_stopped = false;
    let mut visual_stream_capture = visual_stream_capture;

    let flight_result: Result<String> = async {
        ensure_vehicle_landed(&operator, &scenario, "preflight recovery").await?;
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
            .pointer("/world/georeference_origin")
            .and_then(Value::as_object)
            .context("UAV state omitted georeference_origin")?;
        let latitude = json_number(origin, "latitude_degrees")?;
        let longitude = json_number(origin, "longitude_degrees")?;
        let height = json_number(origin, "ellipsoid_height_m")?;
        let mission = serde_json::json!({
            "session_id": scenario.session_id,
            "mission_id": format!("acceptance-{}", uuid::Uuid::now_v7()),
            "expected_world_revision_uri": revision_uri,
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

        let live_result = wait_for_live_stream(
            &operator,
            &live_session_id,
            &live_preview_uri,
            &scenario.stream,
        )
        .await?;
        ensure!(
            live_result
                .pointer("/results/processed_frames")
                .and_then(Value::as_u64)
                .is_some_and(|count| count >= scenario.stream.minimum_live_frames),
            "Stream did not process enough direct live frames: {live_result}"
        );

        // The direct live graph has already proved fresh inference. Composed
        // acceptance keeps it open only until the browser captures that same
        // live session, then releases its DeepStream/TensorRT working set
        // before the independent recording-replay graph starts.
        if let Some(captured) = visual_stream_capture.take() {
            let timeout = Duration::from_secs(
                scenario
                    .takeoff
                    .state_timeout_seconds
                    .saturating_add(scenario.mission.task_timeout_seconds)
                    .saturating_add(scenario.view.timeout_seconds.saturating_mul(3)),
            );
            if tokio::time::timeout(timeout, captured).await.is_err() {
                bail!(
                    "composed visual acceptance did not release the live Stream cleanup hold \
                     within {timeout:?}"
                );
            }
        }
        stop_live_stream_session(&operator, &live_session_id, "live acceptance").await?;
        live_session_stopped = true;

        state = simulation_state(&operator, &scenario).await?;
        let simulation_time_s = state
            .get("simulation_time_s")
            .and_then(Value::as_f64)
            .context("UAV state omitted simulation_time_s")?;
        let replay = &scenario.stream.recording_replay;
        let range_end_s = simulation_time_s - replay.range_lag_seconds;
        let range_start_s = range_end_s - replay.range_duration_seconds;
        ensure!(
            range_start_s >= 0.0,
            "UAV recording has not accumulated enough stable aerial camera history"
        );
        let range_start = (range_start_s * 1_000_000_000.0) as i64;
        let range_end = (range_end_s * 1_000_000_000.0) as i64;
        let freshness_probe_start =
            range_end - (replay.freshness_probe_duration_seconds * 1_000_000_000.0) as i64;

        wait_for_recording_camera_range(
            &operator,
            recording_id,
            &camera_entity,
            freshness_probe_start,
            range_end,
            Duration::from_secs(scenario.recording.live_rows_timeout_seconds),
        )
        .await?;
        let stream_replay = operator
            .task_tool(
                "stream__run_recording",
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
                        "count": replay.maximum_frames
                    },
                    "include_source_clip": true
                }),
                Duration::from_secs(replay.task_timeout_seconds),
            )
            .await?;
        ensure!(
            stream_replay
                .pointer("/summary/processed_frames")
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0),
            "Stream replay processed no Isaac camera frames: {stream_replay}"
        );
        assert_requested_range(&stream_replay, range_start, range_end, "Stream replay")?;
        let governed_artifact_id =
            json_string(&stream_replay, "/results_artifact/artifact_id")?.to_owned();
        ensure!(
            uuid::Uuid::parse_str(&governed_artifact_id)?.get_version_num() == 7,
            "Stream replay result artifact identity must be UUIDv7"
        );
        let stream_results =
            download_governed_json_artifact(conformance, public_base_url, &governed_artifact_id)
                .await?;
        assert_live_recording_snapshot(&stream_results, "Stream replay")?;
        let grounding_uri =
            json_string(&stream_replay, "/results_artifact/artifact_uri")?.to_owned();

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
        assert_requested_range(&reason, range_start, range_end, "Reason")?;
        let reason_artifact_id = json_string(&reason, "/results_artifact/artifact_id")?.to_owned();
        ensure!(
            uuid::Uuid::parse_str(&reason_artifact_id)?.get_version_num() == 7,
            "Reason result artifact identity must be UUIDv7"
        );
        let reason_results =
            download_governed_json_artifact(conformance, public_base_url, &reason_artifact_id)
                .await?;
        assert_live_recording_snapshot(&reason_results, "Reason")?;

        Ok(governed_artifact_id)
    }
    .await;
    let landing_result = ensure_vehicle_landed(&operator, &scenario, "postflight recovery").await;
    let stream_stop_result = if live_session_stopped {
        Ok(())
    } else {
        stop_live_stream_session(&operator, &live_session_id, "postflight cleanup").await
    };
    let governed_artifact_id = match (flight_result, landing_result, stream_stop_result) {
        (Ok(artifact_id), Ok(()), Ok(())) => artifact_id,
        (Err(flight_error), Ok(()), Ok(())) => return Err(flight_error),
        (Ok(_), Err(landing_error), Ok(())) => {
            return Err(landing_error.context("UAV postflight landing failed"));
        }
        (Ok(_), Ok(()), Err(stream_error)) => {
            return Err(stream_error.context("live Stream cleanup failed"));
        }
        (flight, landing, stream) => {
            bail!(
                "UAV acceptance and cleanup had multiple failures: flight={:?}; landing={:?}; \
                 stream={:?}",
                flight.err(),
                landing.err(),
                stream.err()
            );
        }
    };
    assert_concurrent_gpu_workloads(context)?;
    assert_governed_artifact_access(conformance, public_base_url, &governed_artifact_id).await?;

    println!(
        "UAV domain acceptance ok: Google Photorealistic 3D Tiles were resident in Isaac, the \
         showcase pose producer reached ready state, PX4 completed a mission, Stream processed \
         fresh live camera frames and exposed decodable App preview bytes without Recording Hub, \
         Recording Hub retained the world, Stream replay produced a governed artifact, Reason \
         described the flight segment grounded in those detections, an authorized context member \
         previewed it, and an independent context was denied"
    );
    Ok(())
}

async fn recover_live_stream_pipeline(
    operator: &OperatorClient<'_>,
    pipeline_id: &str,
) -> Result<()> {
    let sessions: Vec<LiveSessionView> = serde_json::from_value(
        operator
            .resource("stream://sessions", Duration::from_secs(60))
            .await?,
    )
    .context("decoding visible live Stream sessions")?;
    for session in sessions {
        if session.pipeline_id == pipeline_id && session.lifecycle != LiveSessionLifecycle::Stopped
        {
            eprintln!(
                "preflight recovery: stopping {} live Stream session {} for pipeline {}",
                match session.lifecycle {
                    LiveSessionLifecycle::Starting => "starting",
                    LiveSessionLifecycle::Running => "running",
                    LiveSessionLifecycle::Failed => "failed",
                    LiveSessionLifecycle::Stopped => "stopped",
                },
                session.session_id,
                pipeline_id
            );
            stop_live_stream_session(operator, &session.session_id, "preflight recovery").await?;
        }
    }
    Ok(())
}

async fn stop_live_stream_session(
    operator: &OperatorClient<'_>,
    session_id: &str,
    phase: &str,
) -> Result<()> {
    let output: StopLiveSessionOutput = serde_json::from_value(
        operator
            .call_tool(
                "stream__stop_live_session",
                serde_json::json!({"session_id": session_id}),
            )
            .await
            .with_context(|| format!("{phase}: stopping live Stream session {session_id}"))?,
    )
    .with_context(|| format!("{phase}: decoding stopped live Stream session {session_id}"))?;
    ensure!(
        output.lifecycle == LiveSessionLifecycle::Stopped,
        "{phase}: live Stream session {session_id} did not stop cleanly: {output:?}"
    );
    Ok(())
}

async fn wait_for_live_stream(
    operator: &OperatorClient<'_>,
    session_id: &str,
    preview_uri: &str,
    acceptance: &StreamScenario,
) -> Result<Value> {
    let session_uri = format!("stream://session/{session_id}");
    let results_uri = format!("{session_uri}/results");
    let timeout = Duration::from_secs(acceptance.live_timeout_seconds);
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let session = operator
            .resource(&session_uri, Duration::from_secs(60))
            .await?;
        ensure!(
            json_string(&session, "/lifecycle")? != "failed",
            "live Stream session failed: {session}"
        );
        let results = operator
            .resource(&results_uri, Duration::from_secs(60))
            .await?;
        let preview = operator
            .resource(preview_uri, Duration::from_secs(60))
            .await?;
        let current = serde_json::json!({
            "session": session,
            "results": results,
            "preview": preview
        });

        let enough_frames = current
            .pointer("/results/processed_frames")
            .and_then(Value::as_u64)
            .is_some_and(|count| count >= acceptance.minimum_live_frames);
        let latest_frame = current
            .pointer("/results/frames")
            .and_then(Value::as_array)
            .and_then(|frames| frames.last());
        let fresh = latest_frame
            .and_then(|frame| frame.get("observed_at"))
            .and_then(Value::as_str)
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .is_some_and(|observed_at| {
                let age = Utc::now()
                    .signed_duration_since(observed_at.with_timezone(&Utc))
                    .num_milliseconds();
                age >= 0
                    && age <= i64::try_from(acceptance.maximum_result_age_ms).unwrap_or(i64::MAX)
            });
        let chunks = current
            .pointer("/preview/chunks")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let decodable_preview = validate_live_preview(&chunks).is_ok();
        if enough_frames && fresh && decodable_preview {
            validate_live_preview(&chunks)?;
            return Ok(current);
        }
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "Stream produced no fresh typed results and decodable App preview within \
                 {timeout:?}: {current}"
            );
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn validate_live_preview(chunks: &[Value]) -> Result<()> {
    ensure!(
        chunks.first().and_then(|chunk| chunk.get("keyframe")) == Some(&Value::Bool(true)),
        "live preview must begin at a keyframe"
    );
    let mut last_sequence = None;
    let mut last_timestamp = None;
    for chunk in chunks {
        let sequence = chunk
            .get("sequence")
            .and_then(Value::as_u64)
            .context("live preview chunk omitted sequence")?;
        let timestamp = chunk
            .get("timestamp_us")
            .and_then(Value::as_u64)
            .context("live preview chunk omitted timestamp_us")?;
        if let Some(previous) = last_sequence {
            ensure!(
                sequence == previous + 1,
                "live preview sequence is not contiguous"
            );
        }
        if let Some(previous) = last_timestamp {
            ensure!(
                timestamp >= previous,
                "live preview timestamps moved backwards"
            );
        }
        let encoded = chunk
            .get("data_base64")
            .and_then(Value::as_str)
            .context("live preview chunk omitted data_base64")?;
        let bytes = BASE64_STANDARD
            .decode(encoded)
            .context("live preview chunk is not valid base64")?;
        ensure!(
            bytes.starts_with(&[0, 0, 0, 1]) || bytes.starts_with(&[0, 0, 1]),
            "live preview chunk is not Annex B H.264"
        );
        last_sequence = Some(sequence);
        last_timestamp = Some(timestamp);
    }
    Ok(())
}

async fn ensure_vehicle_landed(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    phase: &str,
) -> Result<()> {
    let timeout = Duration::from_secs(scenario.landing_timeout_seconds);
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let access_error = match simulation_state(operator, scenario).await {
            Ok(state) => {
                let flight_state = json_string(&state, "/vehicles/0/flight_state")?;
                if matches!(flight_state, "landed" | "standby") {
                    return Ok(());
                }
                ensure!(
                    flight_state != "failed",
                    "PX4 entered the failed state during {phase}: {state}"
                );
                if flight_state != "landing" {
                    eprintln!("{phase}: landing UAV from `{flight_state}`");
                    operator
                        .call_tool(
                            "uav-sim__land_vehicle",
                            serde_json::json!({
                                "session_id": scenario.session_id,
                                "vehicle_id": scenario.vehicle_id
                            }),
                        )
                        .await
                        .err()
                } else {
                    None
                }
            }
            Err(error) => {
                eprintln!(
                    "{phase}: UAV control plane is temporarily unavailable while ensuring \
                     landing: {error:#}"
                );
                Some(error)
            }
        };
        if tokio::time::Instant::now() >= deadline {
            if let Some(error) = access_error {
                return Err(error.context(format!(
                    "{phase} could not reach the UAV control plane within {timeout:?}"
                )));
            }
            bail!("PX4 did not reach landed or standby during {phase} within {timeout:?}");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn ensure_world_configured(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<WorldBinding> {
    let initial_state = simulation_state(operator, scenario).await?;
    if initial_state
        .pointer("/world")
        .is_some_and(Value::is_object)
    {
        let revision_uri = json_string(&initial_state, "/world/revision_uri")?.to_owned();
        let simulation_frame_uri =
            json_string(&initial_state, "/world/simulation_frame_uri")?.to_owned();
        verify_published_world(
            operator,
            scenario,
            &revision_uri,
            &simulation_frame_uri,
            None,
        )
        .await?;
        return Ok(WorldBinding {
            revision_uri,
            simulation_frame_uri,
        });
    }
    ensure!(
        json_string(&initial_state, "/lifecycle")? == "unconfigured",
        "UAV session must begin unconfigured or retain the same immutable binding: {initial_state}"
    );
    let tree_digest = hex::encode(Sha256::digest(serde_json::to_vec(&scenario.world.tree)?));
    let world_id = FrameWorldId::new(format!(
        "{}-{}",
        scenario.world.world_id,
        &tree_digest[..16]
    ))?;
    operator
        .call_tool(
            "frames__create_world",
            serde_json::json!({
                "world_id": world_id,
                "display_name": scenario.world.display_name,
                "description": scenario.world.description,
            }),
        )
        .await?;
    let publication = operator
        .call_tool(
            "frames__publish_world",
            serde_json::json!({
                "world_id": world_id,
                "tree": scenario.world.tree,
            }),
        )
        .await?;
    let revision = publication
        .get("revision")
        .cloned()
        .context("Frames publication omitted its immutable revision")?;
    let revision_uri = json_string(&publication, "/revision/revision_uri")?.to_owned();
    let simulation_frame_uri = format!(
        "{revision_uri}/frame/{}",
        scenario.world.simulation_frame_id
    );
    operator
        .call_tool(
            "uav-sim__configure_world",
            serde_json::json!({
                "session_id": scenario.session_id,
                "world_revision": revision,
                "simulation_frame_uri": simulation_frame_uri,
            }),
        )
        .await?;
    verify_published_world(
        operator,
        scenario,
        &revision_uri,
        &simulation_frame_uri,
        Some(&world_id),
    )
    .await?;
    Ok(WorldBinding {
        revision_uri,
        simulation_frame_uri,
    })
}

async fn verify_published_world(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision_uri: &str,
    simulation_frame_uri: &str,
    expected_world_id: Option<&FrameWorldId>,
) -> Result<()> {
    ensure!(
        simulation_frame_uri
            == format!(
                "{revision_uri}/frame/{}",
                scenario.world.simulation_frame_id
            ),
        "UAV immutable binding selects the wrong simulation frame: {simulation_frame_uri}"
    );
    let frame: FrameNode = serde_json::from_str(
        &operator
            .conformance(&["resource", simulation_frame_uri], Duration::from_secs(60))
            .await?,
    )
    .context("decoding the published simulation frame resource")?;
    let expected_frame = scenario
        .world
        .tree
        .frames
        .iter()
        .find(|candidate| candidate.frame_id == scenario.world.simulation_frame_id)
        .expect("validated simulation frame");
    ensure!(
        &frame == expected_frame,
        "published simulation frame disagrees with the scenario: {frame:?}"
    );
    let published_revision: FrameWorldRevision = serde_json::from_str(
        &operator
            .conformance(&["resource", revision_uri], Duration::from_secs(60))
            .await?,
    )
    .context("decoding the published Frames world revision resource")?;
    let mut expected_frames = scenario.world.tree.frames.clone();
    expected_frames.sort_by(|left, right| left.frame_id.cmp(&right.frame_id));
    let mut published_frames = published_revision.tree.frames.clone();
    published_frames.sort_by(|left, right| left.frame_id.cmp(&right.frame_id));
    ensure!(
        published_revision.revision_uri.as_str() == revision_uri
            && published_frames == expected_frames,
        "published Frames world revision disagrees with the complete scenario hierarchy: \
         {published_revision:?}"
    );
    if let Some(expected_world_id) = expected_world_id {
        ensure!(
            &published_revision.world_id == expected_world_id,
            "published Frames world revision changed its run-scoped identity: \
             {published_revision:?}"
        );
    }
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
    let preview_json = download_governed_json_artifact(conformance, base, artifact_id).await?;
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

async fn download_governed_json_artifact(
    conformance: &Path,
    base: &str,
    artifact_id: &str,
) -> Result<Value> {
    let token = gateway_token(conformance, base).await?;
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?
        .get(format!("{base}/artifacts/operator/{artifact_id}/download"))
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("downloading governed JSON artifact {artifact_id}"))?
        .error_for_status()
        .with_context(|| format!("governed JSON artifact {artifact_id} returned an error"))?;
    let media_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    ensure!(
        media_type == "application/json"
            || (media_type.starts_with("application/") && media_type.ends_with("+json")),
        "governed artifact {artifact_id} returned media type `{media_type}`"
    );
    serde_json::from_slice(&response.bytes().await?)
        .with_context(|| format!("governed artifact {artifact_id} contained invalid JSON"))
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
                "Recording Hub exposed no durable live UAV camera samples in range \
                 {range_start}..={range_end} within {timeout:?}: {recording}"
            );
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn assert_live_recording_snapshot(output: &Value, domain: &str) -> Result<()> {
    let sources = output
        .pointer("/source_snapshot/sources")
        .and_then(Value::as_array)
        .with_context(|| format!("{domain} omitted its governed recording source snapshot"))?;
    ensure!(
        sources.iter().any(|source| {
            source.get("kind").and_then(Value::as_str) == Some("live_ingest_part")
        }),
        "{domain} did not analyze an acknowledged live ingest part before archive rollover: \
         {output}"
    );
    Ok(())
}

fn assert_requested_range(output: &Value, start: i64, end: i64, domain: &str) -> Result<()> {
    ensure!(
        output
            .pointer("/summary/requested_start_index")
            .and_then(Value::as_i64)
            == Some(start)
            && output
                .pointer("/summary/requested_end_index")
                .and_then(Value::as_i64)
                == Some(end),
        "{domain} did not analyze the requested near-live recording range: {output}"
    );
    Ok(())
}

fn assert_concurrent_gpu_workloads(context: &str) -> Result<()> {
    for deployment in ["uav-sim", "view-mcp", "stream-mcp", "reason-mcp"] {
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

fn assert_world_ready(
    state: &Value,
    scenario: &UavAcceptanceScenario,
    revision_uri: &str,
    simulation_frame_uri: &str,
) -> Result<()> {
    ensure!(
        matches!(
            json_string(state, "/lifecycle")?,
            "ready" | "running" | "paused"
        ),
        "UAV session is not ready: {state}"
    );
    ensure!(
        json_string(state, "/world/revision_uri")? == revision_uri
            && json_string(state, "/world/simulation_frame_uri")? == simulation_frame_uri
            && json_string(state, "/world/spec_sha256")?.len() == 64,
        "UAV session uses the wrong immutable Frames world: {state}"
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
        json_string(state, "/pose_publication/protocol_schema")?
            == "veoveo.io/simulation-view-pose/v1"
            && json_string(state, "/pose_publication/lifecycle")? == "ready"
            && json_string(state, "/pose_publication/entity_table_digest")?.starts_with("sha256:")
            && state
                .pointer("/pose_publication/sent_snapshots")
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0),
        "Simulation View pose publication is not ready: {state}"
    );
    Ok(())
}

async fn wait_for_world_ready(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision_uri: &str,
    simulation_frame_uri: &str,
    timeout: Duration,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(operator, scenario).await?;
        let lifecycle = json_string(&state, "/lifecycle")?;
        ensure!(
            lifecycle != "failed",
            "UAV simulation failed while loading its frame world: {state}"
        );
        if matches!(lifecycle, "ready" | "running" | "paused") {
            assert_world_ready(&state, scenario, revision_uri, simulation_frame_uri)?;
            return Ok(state);
        }
        if tokio::time::Instant::now() >= deadline {
            bail!("UAV frame world was not ready within {timeout:?}; final state: {state}");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
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

fn assert_georeference_origin(state: &Value, scenario: &UavAcceptanceScenario) -> Result<()> {
    let origin = state
        .pointer("/world/georeference_origin")
        .and_then(Value::as_object)
        .context("UAV state omitted georeference_origin")?;
    let expected_origin = scenario.world.origin()?;
    for (key, expected) in [
        ("latitude_degrees", expected_origin.latitude_degrees),
        ("longitude_degrees", expected_origin.longitude_degrees),
        ("ellipsoid_height_m", expected_origin.ellipsoid_height_m),
    ] {
        let actual = json_number(origin, key)?;
        ensure!(
            (actual - expected).abs() <= 1e-9,
            "UAV state {key} {actual} disagrees with scenario origin {expected}"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical_scenario() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../showcase/uav-sim/scenarios/new-york-aerial.json")
    }

    #[test]
    fn canonical_mission_is_runtime_loaded_and_validated() {
        let scenario = UavAcceptanceScenario::load(&canonical_scenario()).unwrap();
        assert_eq!(scenario.schema, "veoveo.uav-sim-acceptance/v9");
        assert_eq!(scenario.session_id, "uav-showcase");
        assert_eq!(scenario.world.world_id.as_str(), "uav-showcase-new-york");
        assert_eq!(scenario.world.tree.frames.len(), 6);
        let origin = scenario.world.origin().unwrap();
        assert_eq!(origin.latitude_degrees, 40.758);
        assert_eq!(origin.longitude_degrees, -73.9855);
        assert_eq!(origin.ellipsoid_height_m, -17.0);
        assert_eq!(scenario.takeoff.relative_altitude_m, 300.0);
        assert_eq!(scenario.mission.speed_mps, 3.0);
        assert_eq!(scenario.recording.live_rows_timeout_seconds, 120);
        assert_eq!(scenario.camera.aerial_detail.minimum_dynamic_range, 8);
        assert_eq!(scenario.stream.recording_replay.range_lag_seconds, 1.0);
        assert_eq!(
            scenario
                .stream
                .recording_replay
                .freshness_probe_duration_seconds,
            1.0
        );
        assert!(!scenario.reason.prompt.is_empty());
        assert_eq!(scenario.reason.maximum_frames, 6);
        assert_eq!(scenario.view.camera.width_px, 640);
        assert_eq!(scenario.view.minimum_mission_pose_delta, 30);
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
