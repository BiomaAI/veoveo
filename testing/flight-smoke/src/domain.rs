use std::collections::BTreeSet;
use std::process::Stdio;

use crate::wire::{
    LiveSessionLifecycle, LiveSessionView, StartLiveSessionOutput, StopLiveSessionOutput,
};
use crate::{browser, support::*};
use anyhow::ensure;
use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{ffi::OsString, fs, path::Path, time::Duration};
use tokio::sync::oneshot;
use veoveo_mcp_contract::{
    FrameBasis, FrameId, FrameNode, FrameParentTransform, FrameWorldId, FrameWorldRevision,
    FrameWorldTree, LiveCameraDescriptor, LiveCameraHealth, LiveStreamProductLifecycle,
    LiveStreamProductState, Wgs84Position,
};

mod artifacts;
mod client;
mod scenario;
mod showcase;
mod stream;
mod world;
use artifacts::*;
use client::*;
use scenario::*;
pub(crate) use showcase::{uav_showcase_up, uav_showcase_verify};
use stream::*;
use world::*;

const NAMESPACE: &str = "veoveo";
const GOOGLE_PHOTOREALISTIC_3D_TILES_ASSET_ID: u64 = 2_275_207;
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

pub(crate) async fn uav_sim_verify(
    conformance: &Path,
    scenario_path: &Path,
    context: &str,
    public_base_url: &str,
) -> Result<()> {
    uav_sim_verify_with_visual_hold(conformance, scenario_path, context, public_base_url, None)
        .await
}

struct UavVisualHolds {
    stream_capture_complete: oneshot::Receiver<()>,
    moving_recording_capture_complete: oneshot::Receiver<()>,
}

async fn uav_sim_verify_with_visual_hold(
    conformance: &Path,
    scenario_path: &Path,
    context: &str,
    public_base_url: &str,
    visual_holds: Option<UavVisualHolds>,
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
        "uav-sim__list_active_vehicle_control_grants",
        "uav-sim__prepare_vehicle_mission",
        "uav-sim__execute_vehicle_mission_plan",
        "map__route",
        "map__prepare_route_handoff",
        "stream__start_live_session",
        "stream__stop_live_session",
        "stream__run_recording",
        "reason__analyze_recording",
        "recording__create_recording_projection",
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
    state = wait_for_recording_catalog(&operator, &scenario, Duration::from_secs(30)).await?;
    let control_grant = ensure_operator_control_grant(&operator, &scenario).await?;
    let recording_uri = json_string(&state, "/recordings/0/recording_uri")?.to_owned();
    let recording_id = recording_uri
        .strip_prefix("recording://recordings/")
        .context("UAV state returned a non-canonical recording URI")?;
    ensure!(
        uuid::Uuid::parse_str(recording_id)?.get_version_num() == 7,
        "UAV recording identity must be UUIDv7"
    );
    let recording_catalog_entry = operator
        .resource(&recording_uri, Duration::from_secs(60))
        .await?;
    let dataset_id = json_string(&recording_catalog_entry, "/dataset_id")?.to_owned();
    ensure!(
        uuid::Uuid::parse_str(&dataset_id)?.get_version_num() == 7,
        "UAV recording dataset identity must be UUIDv7"
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
    let live = prepare_live_stream_pipeline(&operator, &scenario.stream.live_pipeline_id).await?;
    let live_session_id = live.session_id;
    let live_preview_uri = live.preview_uri;
    let owned_live_session = live.owned_by_acceptance;
    let mut owned_live_session_stopped = false;
    let (mut visual_stream_capture, mut moving_recording_capture) = match visual_holds {
        Some(holds) => (
            Some(holds.stream_capture_complete),
            Some(holds.moving_recording_capture_complete),
        ),
        None => (None, None),
    };

    let flight_result: Result<String> = async {
        ensure_vehicle_landed(&operator, &scenario, "preflight recovery").await?;
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
        state = wait_for_native_camera_stream(
            &operator,
            Duration::from_secs(scenario.camera.stream_timeout_seconds),
            &scenario,
        )
        .await?;

        let current_position: Wgs84Position = serde_json::from_value(
            state
                .pointer("/vehicles/0/wgs84")
                .cloned()
                .context("UAV state omitted the current vehicle WGS84 position")?,
        )
        .context("UAV state returned an invalid current vehicle WGS84 position")?;
        let target_position =
            nearby_mission_position(&current_position, scenario.mission.longitude_offset_degrees)?;
        let (mobility_profile_id, mobility_profile_version) =
            parse_mobility_profile_uri(json_string(&control_grant, "/map_mobility_profile_uri")?)?;
        let route = operator
            .task_tool(
                "map__route",
                serde_json::json!({
                    "mobility_profile_id": mobility_profile_id,
                    "mobility_profile_version": mobility_profile_version,
                    "origin": {
                        "kind": "position",
                        "position": map_position(&current_position)
                    },
                    "destination": {
                        "kind": "position",
                        "position": map_position(&target_position)
                    },
                    "waypoints": [],
                    "departure_time": Utc::now(),
                    "objective": { "kind": "shortest" },
                    "constraints": {},
                    "alternatives": 0,
                    "data_policy": {
                        "allow_planning_advisory": true,
                        "allow_stale_operational_data": false,
                        "required_map_families": ["aviation"]
                    }
                }),
                Duration::from_secs(scenario.mission.task_timeout_seconds),
            )
            .await?;
        let map_route = operator
            .call_tool(
                "map__prepare_route_handoff",
                serde_json::json!({
                    "route_id": json_string(&route, "/route_id")?
                }),
            )
            .await?;
        let mission_timeout = governed_mission_timeout(
            &route,
            scenario.mission.speed_mps,
            scenario.mission.task_timeout_seconds,
        )?;
        let mission_id = format!("acceptance-{}", uuid::Uuid::now_v7());
        let plan = operator
            .call_tool(
                "uav-sim__prepare_vehicle_mission",
                serde_json::json!({
                    "session_id": scenario.session_id,
                    "mission_id": mission_id,
                    "vehicle_id": scenario.vehicle_id,
                    "expected_world_revision_uri": revision_uri,
                    "map_route": map_route,
                    "speed_mps": scenario.mission.speed_mps,
                    "hold_seconds_at_destination": scenario.mission.hold_seconds
                }),
            )
            .await?;
        let mission_output = operator
            .task_tool(
                "uav-sim__execute_vehicle_mission_plan",
                serde_json::json!({
                    "plan_id": json_string(&plan, "/plan_id")?,
                    "expected_revision": plan
                        .get("revision")
                        .and_then(Value::as_u64)
                        .context("prepared UAV mission plan omitted its revision")?
                }),
                mission_timeout,
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
        if owned_live_session {
            stop_live_stream_session(&operator, &live_session_id, "live acceptance").await?;
            owned_live_session_stopped = true;
        }

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
            &dataset_id,
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

        if let Some(captured) = moving_recording_capture.take() {
            let timeout = Duration::from_secs(scenario.view.timeout_seconds.saturating_add(30));
            if tokio::time::timeout(timeout, captured).await.is_err() {
                bail!(
                    "composed visual acceptance did not release the moving Rerun capture hold \
                     within {timeout:?}"
                );
            }
        }

        Ok(governed_artifact_id)
    }
    .await;
    let landing_result = ensure_vehicle_landed(&operator, &scenario, "postflight recovery").await;
    let stream_stop_result = if !owned_live_session || owned_live_session_stopped {
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

#[cfg(test)]
#[path = "domain/tests.rs"]
mod tests;
