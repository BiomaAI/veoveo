use std::path::PathBuf;

use super::*;

fn canonical_scenario() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../showcase/uav-sim/scenarios/new-york-aerial.json")
}

fn live_session(session_id: &str, pipeline_id: &str, lifecycle: &str) -> LiveSessionView {
    serde_json::from_value(serde_json::json!({
        "session_id": session_id,
        "session_uri": format!("stream://session/{session_id}"),
        "results_uri": format!("stream://session/{session_id}/results"),
        "pipeline_id": pipeline_id,
        "pipeline_uri": format!("stream://pipeline/{pipeline_id}"),
        "ingress": {
            "transport": "rtp_h264_udp",
            "host": "stream-mcp",
            "port": 5004,
            "payload_type": 96,
            "clock_rate": 90000,
            "caps": "application/x-rtp,media=video,encoding-name=H264"
        },
        "video": {
            "codec": "avc1.640028",
            "width": 640,
            "height": 480,
            "frame_rate": 2,
            "expected_bitrate_bps": 4000000
        },
        "preview_uri": format!("stream://session/{session_id}/preview"),
        "lifecycle": lifecycle,
        "started_at": "2026-08-07T18:00:00Z",
        "received_video_frames": 12,
        "processed_frames": 12
    }))
    .unwrap()
}

#[test]
fn canonical_mission_is_runtime_loaded_and_validated() {
    let scenario = UavAcceptanceScenario::load(&canonical_scenario()).unwrap();
    assert_eq!(scenario.schema, "veoveo.uav-sim-acceptance/v11");
    assert_eq!(
        scenario.map_mobility_profile_uri,
        "map://mobility-profile/mobility-019ffdb2-0598-7476-96d3-f3d7b0769f9e/1"
    );
    assert_eq!(scenario.session_id, "uav-showcase");
    assert_eq!(scenario.world.world_id.as_str(), "uav-showcase-new-york");
    assert_eq!(scenario.world.tree.frames.len(), 15);
    for vehicle in 1..=4 {
        assert!(
            scenario
                .world
                .tree
                .frames
                .iter()
                .any(|frame| { frame.frame_id.as_str() == format!("uav-{vehicle}-body") })
        );
    }
    let origin = scenario.world.origin().unwrap();
    assert_eq!(origin.latitude_degrees, 40.758);
    assert_eq!(origin.longitude_degrees, -73.9855);
    assert_eq!(origin.ellipsoid_height_m, -17.0);
    assert_eq!(scenario.takeoff.relative_altitude_m, 197.0);
    assert_eq!(scenario.takeoff.state_timeout_seconds, 1800);
    assert_eq!(scenario.mission.longitude_offset_degrees, 0.0002);
    assert_eq!(scenario.mission.speed_mps, 20.0);
    assert_eq!(scenario.mission.task_timeout_seconds, 1800);
    assert_eq!(scenario.recording.live_rows_timeout_seconds, 120);
    assert_eq!(scenario.camera.stream_timeout_seconds, 60);
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
    assert_eq!(scenario.view.minimum_mission_sensor_frames, 10);
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

#[test]
fn stream_product_acceptance_requires_one_ready_tiled_product() {
    let products: Vec<LiveStreamProductState> = serde_json::from_value(serde_json::json!([
        {"streamProductId":"camera-atlas","cameraRegions":[
            {"cameraId":"follow","xPx":0,"yPx":0,"widthPx":1280,"heightPx":720},
            {"cameraId":"chase","xPx":1280,"yPx":0,"widthPx":1280,"heightPx":720}
         ],"codedWidthPx":2560,"codedHeightPx":720,"lifecycle":"ready",
         "activeViewers":0,"connectedViewers":0,"nvencSessions":1,"encodedFrames":12,
         "sourceToRenderSamples":120,"lastFrameAt":"2026-08-07T18:00:00Z","visible":true}
    ]))
    .unwrap();
    assert!(ready_camera_product_set_matches_contract(&products));
    assert!(camera_product_set_matches_contract(&products));
}

#[test]
fn stream_product_acceptance_allows_many_viewers_on_one_product() {
    let shared: Vec<LiveStreamProductState> = serde_json::from_value(serde_json::json!([
        {"streamProductId":"camera-atlas","cameraRegions":[
            {"cameraId":"follow","xPx":0,"yPx":0,"widthPx":1280,"heightPx":720}
         ],"codedWidthPx":1280,"codedHeightPx":720,
         "lifecycle":"ready","activeViewers":25,
         "connectedViewers":25,"nvencSessions":1,"encodedFrames":12,
         "sourceToRenderP95Microseconds":18000,"sourceToRenderSamples":120,
         "lastFrameAt":"2026-08-07T18:00:00Z","visible":true}
    ]))
    .unwrap();
    assert!(ready_camera_product_set_matches_contract(&shared));
    assert!(camera_product_set_matches_contract(&shared));
}

#[test]
fn stream_product_acceptance_rejects_duplicate_camera_products() {
    let duplicate: Vec<LiveStreamProductState> = serde_json::from_value(serde_json::json!([
        {"streamProductId":"camera-atlas-a","cameraRegions":[
            {"cameraId":"follow","xPx":0,"yPx":0,"widthPx":1280,"heightPx":720}
         ],"codedWidthPx":1280,"codedHeightPx":720,"lifecycle":"starting",
         "activeViewers":0,"connectedViewers":0,"nvencSessions":0,"encodedFrames":0,
         "sourceToRenderSamples":0},
        {"streamProductId":"camera-atlas-b","cameraRegions":[
            {"cameraId":"follow","xPx":0,"yPx":0,"widthPx":1280,"heightPx":720}
         ],"codedWidthPx":1280,"codedHeightPx":720,"lifecycle":"starting",
         "activeViewers":0,"connectedViewers":0,"nvencSessions":0,"encodedFrames":0,
         "sourceToRenderSamples":0}
    ]))
    .unwrap();
    assert!(!ready_camera_product_set_matches_contract(&duplicate));
    assert!(!camera_product_set_matches_contract(&duplicate));
}

#[test]
fn stream_preflight_reuses_one_visible_session_without_claiming_ownership() {
    let sessions = vec![
        live_session("stopped", "traffic", "stopped"),
        live_session("active", "traffic", "running"),
        live_session("other", "another-pipeline", "running"),
    ];
    let selected = reusable_live_stream_session(&sessions, "traffic")
        .unwrap()
        .unwrap();
    assert_eq!(selected.session_id, "active");
}

#[test]
fn stream_preflight_rejects_multiple_visible_active_sessions() {
    let sessions = vec![
        live_session("first", "traffic", "starting"),
        live_session("second", "traffic", "running"),
    ];
    assert!(reusable_live_stream_session(&sessions, "traffic").is_err());
}

#[test]
fn acceptance_mission_is_bounded_from_the_current_authorized_pose() {
    let current = Wgs84Position {
        latitude_degrees: 40.758,
        longitude_degrees: -73.9855,
        ellipsoid_height_m: 283.0,
    };
    let target = nearby_mission_position(&current, 0.0002).unwrap();
    assert_eq!(target.latitude_degrees, current.latitude_degrees);
    assert_eq!(target.longitude_degrees, -73.9853);
    assert_eq!(target.ellipsoid_height_m, current.ellipsoid_height_m);
}

#[test]
fn governed_mission_budget_tracks_the_authoritative_route_cost() {
    let route = serde_json::json!({
        "summary": {"distance": 580.0, "duration": 29.0}
    });
    assert_eq!(
        governed_mission_timeout(&route, 20.0, 1800).unwrap(),
        Duration::from_secs(118)
    );

    let excessive = serde_json::json!({
        "summary": {"distance": 20_000.0, "duration": 1_000.0}
    });
    assert!(governed_mission_timeout(&excessive, 20.0, 1800).is_err());
}

#[test]
fn native_sensor_stream_requires_nvenc_access_units() {
    let ready = serde_json::json!({
        "lifecycle":"ready",
        "transport":"rtsp_rtp",
        "codec":"h264",
        "encoder":"nvidia_nvenc",
        "frames_observed":13,
        "last_access_unit_bytes":4821,
        "last_frame_keyframe":false
    });
    assert!(sensor_camera_is_started(&ready));

    let no_access_unit = serde_json::json!({
        "lifecycle":"ready",
        "transport":"rtsp_rtp",
        "codec":"h264",
        "encoder":"nvidia_nvenc",
        "frames_observed":13,
        "last_access_unit_bytes":0,
        "last_frame_keyframe":false
    });
    assert!(!sensor_camera_is_started(&no_access_unit));
}

#[test]
fn live_preview_orders_reordered_h264_by_decode_sequence() {
    let access_unit = BASE64_STANDARD.encode([0, 0, 0, 1, 0x65]);
    let chunks = vec![
        serde_json::json!({
            "sequence": 0,
            "timestamp_us": 200_000,
            "keyframe": true,
            "data_base64": access_unit,
        }),
        serde_json::json!({
            "sequence": 1,
            "timestamp_us": 100_000,
            "keyframe": false,
            "data_base64": access_unit,
        }),
    ];
    validate_live_preview(&chunks).expect("AVC presentation reordering is valid");
}
