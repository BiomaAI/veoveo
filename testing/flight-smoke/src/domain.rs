use std::collections::BTreeSet;
use std::process::Stdio;

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
use veoveo_frames_mcp::contract::{
    FrameBasis, FrameId, FrameNode, FrameParentTransform, FrameWorldId, FrameWorldRevision,
    FrameWorldRevisionUri, FrameWorldTree, Wgs84Position, WorldFrameUri,
};
use veoveo_map_mcp::contract::MapMobilityProfileUri;
use veoveo_recording_contract::RecordingId;
use veoveo_stream_mcp::contract::{
    LiveSessionLifecycle, LiveSessionView, StartLiveSessionOutput, StopLiveSessionOutput,
};
use veoveo_types::ScopeDefinition;

use veoveo_uav_sim_mcp::contract::{
    LiveCameraHealth, LiveStreamProductLifecycle, LiveStreamProductState, UavScope,
};

mod artifacts;
mod client;
mod control_grants;
mod readiness;
mod recording;
pub(crate) use recording::verify as uav_recording_verify;
mod route;
mod scenario;
pub(crate) use route::verify as uav_route_verify;
mod showcase;
mod stream;
pub(crate) use stream::verify as uav_stream_verify;
mod world;
mod world_publication;
use artifacts::*;
use client::*;
use scenario::*;
pub(crate) use showcase::{uav_showcase_up, uav_showcase_verify};
use stream::*;
use world::*;
pub(crate) use world_publication::uav_world_publish;

const GOOGLE_PHOTOREALISTIC_3D_TILES_ASSET_ID: u64 = 2_275_207;
fn preflight_flight_authority(installation: &InstalledTarget) -> Result<()> {
    installation.operator.validate_credentials()?;
    installation.administrator()?.validate_credentials()?;
    ensure!(
        installation.operator.comparison_context.is_some(),
        "flight artifact isolation requires operator.comparisonContext"
    );
    for scope in [UavScope::Read, UavScope::Control, UavScope::Stream] {
        ensure!(
            installation.operator.scopes.contains(scope.name()),
            "installation operator.scopes must include {scope}"
        );
    }
    ensure!(
        installation
            .administrator()?
            .scopes
            .contains(UavScope::Admin.name()),
        "installation administrator.scopes must include {}",
        UavScope::Admin
    );
    Ok(())
}

// TODO(foundations): Complete composed flight and timing acceptance on the
// rebuilt reference installation; focused live, replay and Reason checks pass.
pub(crate) async fn uav_sim_verify(
    conformance: &Path,
    scenario_path: &Path,
    installation: &InstalledTarget,
) -> Result<()> {
    uav_sim_verify_with_visual_hold(conformance, scenario_path, installation, None).await
}

struct UavVisualHolds {
    phases: UavVisualPhases,
    stream_capture_complete: oneshot::Receiver<()>,
    moving_recording_capture_complete: oneshot::Receiver<()>,
}

struct UavVisualPhases {
    takeoff_ready: oneshot::Sender<()>,
    takeoff_capture_complete: oneshot::Receiver<()>,
    mission_ready: oneshot::Sender<()>,
}

async fn uav_sim_verify_with_visual_hold(
    conformance: &Path,
    scenario_path: &Path,
    installation: &InstalledTarget,
    visual_holds: Option<UavVisualHolds>,
) -> Result<()> {
    let scenario = UavAcceptanceScenario::load(scenario_path)?;
    assert_executable(conformance)?;
    preflight_flight_authority(installation)?;
    let context = &installation.target.kubernetes.context;
    let namespace = &installation.target.kubernetes.namespace;

    run_checked(
        Path::new("kubectl"),
        ["--context", context, "cluster-info"].map(OsString::from),
        [],
    )
    .context("UAV live acceptance requires its Kubernetes cluster")?;
    assert_concurrent_gpu_workloads(context, namespace)?;

    let operator = OperatorClient {
        conformance,
        installation,
    };
    let tools = operator
        .conformance(&["tools"], Duration::from_secs(60))
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
    ] {
        contains(&tools, tool)?;
    }

    route::preflight(&operator, &scenario).await?;

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
    let mut flight_control_started = false;
    let (mut visual_phases, mut visual_stream_capture, mut moving_recording_capture) =
        match visual_holds {
            Some(holds) => (
                Some(holds.phases),
                Some(holds.stream_capture_complete),
                Some(holds.moving_recording_capture_complete),
            ),
            None => (None, None, None),
        };

    let flight_result: Result<veoveo_artifact_contract::ArtifactId> = async {
        // Qualify independent live inference before any landing or takeoff work.
        // The later check still proves freshness after the mission.
        wait_for_live_stream(
            &operator,
            &live_session_id,
            &live_preview_uri,
            &scenario.stream,
        )
        .await?;
        eprintln!("UAV live Stream prerequisite passed before flight commands");
        let control_grant = ensure_operator_control_grant(&operator, &scenario).await?;
        flight_control_started = true;
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
        wait_for_takeoff(&operator, &scenario, &revision_uri).await?;
        state = wait_for_native_camera_stream(
            &operator,
            Duration::from_secs(scenario.camera.stream_timeout_seconds),
            &scenario,
        )
        .await?;

        // Capture this run's acknowledged takeoff before starting its mission.
        // Existing simulator activity cannot satisfy the visual phase gate.
        let mission_ready = if let Some(phases) = visual_phases.take() {
            let _ = phases.takeoff_ready.send(());
            wait_for_visual_capture(
                phases.takeoff_capture_complete,
                Duration::from_secs(scenario.view.timeout_seconds),
                "takeoff",
            )
            .await?;
            Some(phases.mission_ready)
        } else {
            None
        };

        let current_position: Wgs84Position = serde_json::from_value(
            state
                .pointer("/vehicles/0/wgs84")
                .cloned()
                .context("UAV state omitted the current vehicle WGS84 position")?,
        )
        .context("UAV state returned an invalid current vehicle WGS84 position")?;
        let target_position =
            nearby_mission_position(&current_position, scenario.mission.longitude_offset_degrees)?;
        let mobility_profile = &control_grant.map_mobility_profile_uri;
        let route = route::plan(
            &operator,
            mobility_profile,
            &current_position,
            &target_position,
            Duration::from_secs(scenario.mission.task_timeout_seconds),
        )
        .await?;
        route::execute(&operator, &scenario, &revision_uri, &route).await?;
        if let Some(ready) = mission_ready {
            let _ = ready.send(());
        }

        wait_for_live_stream(
            &operator,
            &live_session_id,
            &live_preview_uri,
            &scenario.stream,
        )
        .await?;

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
            wait_for_visual_capture(captured, timeout, "live Stream").await?;
        }
        if owned_live_session {
            stop_live_stream_session(&operator, &live_session_id, "live acceptance").await?;
            owned_live_session_stopped = true;
        }

        let governed_artifact_id = recording::analyze(&operator, &scenario)
            .await?
            .stream_artifact_id;

        if let Some(captured) = moving_recording_capture.take() {
            let timeout = Duration::from_secs(scenario.view.timeout_seconds.saturating_add(30));
            wait_for_visual_capture(captured, timeout, "moving Rerun").await?;
        }

        route::return_to_launch(
            &operator,
            &scenario,
            &revision_uri,
            &control_grant.map_mobility_profile_uri,
        )
        .await?;
        Ok(governed_artifact_id)
    }
    .await;
    if let Err(error) = &flight_result {
        eprintln!("UAV acceptance failed; starting owned postflight cleanup: {error:#}");
    }
    let landing_result = if flight_control_started {
        ensure_vehicle_landed(&operator, &scenario, "postflight recovery").await
    } else {
        Ok(())
    };
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
    route::assert_landed_at_launch(&operator, &scenario, &revision_uri).await?;
    assert_concurrent_gpu_workloads(context, namespace)?;
    assert_governed_artifact_access(conformance, installation, &governed_artifact_id).await?;

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

async fn wait_for_visual_capture(
    captured: oneshot::Receiver<()>,
    timeout: Duration,
    phase: &str,
) -> Result<()> {
    tokio::time::timeout(timeout, captured)
        .await
        .with_context(|| format!("composed {phase} capture did not finish within {timeout:?}"))?
        .with_context(|| format!("composed {phase} capture failed before acknowledgement"))
}

fn assert_concurrent_gpu_workloads(context: &str, namespace: &str) -> Result<()> {
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
    Ok(())
}

#[cfg(test)]
#[path = "domain/tests.rs"]
mod tests;
