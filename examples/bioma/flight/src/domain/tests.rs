use std::path::PathBuf;

use super::*;

pub(super) fn canonical_scenario() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../showcase/uav-sim/scenarios/new-york-aerial.json")
}

#[tokio::test]
async fn visual_capture_holds_require_acknowledgement_and_have_a_deadline() {
    let (sender, receiver) = oneshot::channel();
    sender.send(()).unwrap();
    wait_for_visual_capture(receiver, Duration::from_secs(1), "fixture")
        .await
        .unwrap();

    let (sender, receiver) = oneshot::channel();
    drop(sender);
    let error = wait_for_visual_capture(receiver, Duration::from_secs(1), "fixture")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("failed before acknowledgement"));

    let (_sender, receiver) = oneshot::channel();
    let error = wait_for_visual_capture(receiver, Duration::from_millis(1), "fixture")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("did not finish within"));
}

#[test]
fn takeoff_waits_for_the_selected_aircraft_to_reach_altitude() {
    use veoveo_uav_sim_mcp::contract::{SimulationState, VehicleFlightState, VehicleId};

    let mut state: SimulationState =
        serde_json::from_str(include_str!("../../tests/fixtures/world-ready.json")).unwrap();
    let selected = state.vehicles[0].vehicle_id.clone();
    state.vehicles[0].flight_state = VehicleFlightState::Flying;
    state.vehicles[0].enu.up_m = 1.7;
    let mut other = state.vehicles[0].clone();
    other.vehicle_id = VehicleId::parse("other-uav").unwrap();
    other.enu.up_m = 300.0;
    state.vehicles.insert(0, other);
    assert!(!takeoff_is_ready(&state, &selected, 192.0).unwrap());
    state.vehicles[1].enu.up_m = 192.0;
    assert!(takeoff_is_ready(&state, &selected, 192.0).unwrap());
    state.vehicles[1].flight_state = VehicleFlightState::Armed;
    assert!(!takeoff_is_ready(&state, &selected, 192.0).unwrap());
    state.vehicles[1].flight_state = VehicleFlightState::Failed;
    assert!(takeoff_is_ready(&state, &selected, 192.0).is_err());
    state.vehicles[1].flight_state = VehicleFlightState::Flying;
    state.vehicles[1].enu.up_m = f64::NAN;
    assert!(takeoff_is_ready(&state, &selected, 192.0).is_err());
    state.vehicles.remove(1);
    assert!(takeoff_is_ready(&state, &selected, 192.0).is_err());
}

pub(super) fn live_session(
    session_number: u8,
    pipeline_id: &str,
    lifecycle: &str,
) -> LiveSessionView {
    let session_id = format!("01983da0-0000-7000-8000-{session_number:012x}");
    serde_json::from_value(serde_json::json!({
        "sessionId": session_id,
        "sessionUri": format!("stream://session/{session_id}"),
        "resultsUri": format!("stream://session/{session_id}/results"),
        "pipelineId": pipeline_id,
        "pipelineUri": format!("stream://pipeline/{pipeline_id}"),
        "ingress": {
            "transport": "rtp_h264_udp",
            "host": "stream-mcp",
            "port": 5004,
            "payloadType": 96,
            "clockRate": 90000,
            "caps": "application/x-rtp,media=video,encoding-name=H264"
        },
        "video": {
            "codec": "avc1.640028",
            "width": 640,
            "height": 480,
            "frameRate": 2,
            "expectedBitrateBps": 4000000
        },
        "previewUri": format!("stream://session/{session_id}/preview"),
        "lifecycle": lifecycle,
        "startedAt": "2026-08-07T18:00:00Z",
        "receivedVideoFrames": 12,
        "processedFrames": 12
    }))
    .unwrap()
}

#[test]
fn canonical_mission_is_runtime_loaded_and_validated() {
    let scenario = UavAcceptanceScenario::load(&canonical_scenario()).unwrap();
    assert_eq!(scenario.schema, "veoveo.ai/uav-sim-acceptance/v13");
    let mut retired = scenario.clone();
    retired.schema = "veoveo.ai/uav-sim-acceptance/v12".into();
    assert!(retired.validate().is_err());
    assert_eq!(
        scenario.map_mobility_profile_uri.as_str(),
        "map://mobility-profile/mobility-019ffdb2-0598-7476-96d3-f3d7b0769f9e/1"
    );
    assert_eq!(scenario.session_id.as_str(), "uav-showcase");
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
    assert_eq!(scenario.camera.stream_timeout_seconds, 60);
    assert_eq!(scenario.stream.recording_replay.range_lag_seconds, 1.0);
    assert!(!scenario.reason.prompt.is_empty());
    assert_eq!(scenario.reason.maximum_frames, 6);
    assert_eq!(scenario.view.camera.width_px, 640);
    assert_eq!(scenario.view.minimum_mission_sensor_frames, 10);
}

#[test]
fn mission_rejects_noncanonical_map_profile_references_on_decode() {
    let good: Value = serde_json::from_slice(&fs::read(canonical_scenario()).unwrap()).unwrap();
    for uri in [
        "map://mobility-profile/alias/1",
        "map://mobility-profile/mobility-019ffdb2-0598-7476-96d3-f3d7b0769f9e/0",
        "map://mobility-profile/mobility-019ffdb2-0598-7476-96d3-f3d7b0769f9e/01",
        "map://mobility-profile/mobility-019ffdb2-0598-7476-96d3-f3d7b0769f9e/1?version=2",
    ] {
        let mut wire = good.clone();
        wire["mapMobilityProfileUri"] = uri.into();
        assert!(serde_json::from_value::<UavAcceptanceScenario>(wire).is_err());
    }
}

#[test]
fn mission_file_is_outside_the_isaac_image_build_context() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
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
        live_session(1, "traffic", "stopped"),
        live_session(2, "traffic", "running"),
        live_session(3, "another-pipeline", "running"),
    ];
    let selected = reusable_live_stream_session(&sessions, &"traffic".parse().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(
        selected.session_id().to_string(),
        "01983da0-0000-7000-8000-000000000002"
    );
}

#[test]
fn stream_preflight_rejects_multiple_visible_active_sessions() {
    let sessions = vec![
        live_session(1, "traffic", "starting"),
        live_session(2, "traffic", "running"),
    ];
    assert!(reusable_live_stream_session(&sessions, &"traffic".parse().unwrap()).is_err());
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
    use veoveo_map_mcp::contract::{Meters, Ratio, RouteCost, Seconds};
    let route = RouteCost {
        distance: Meters::new(580.0).unwrap(),
        duration: Seconds::new(29.0).unwrap(),
        energy: None,
        fuel: None,
        monetary_minor_units: None,
        risk: Ratio::new(0.0).unwrap(),
    };
    assert_eq!(
        governed_mission_timeout(&route, 20.0, 1800).unwrap(),
        Duration::from_secs(118)
    );

    let excessive = RouteCost {
        distance: Meters::new(20_000.0).unwrap(),
        duration: Seconds::new(1_000.0).unwrap(),
        ..route.clone()
    };
    assert!(governed_mission_timeout(&excessive, 20.0, 1800).is_err());
    for invalid_speed in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(governed_mission_timeout(&route, invalid_speed, 1800).is_err());
    }
}

#[test]
fn native_sensor_stream_requires_nvenc_access_units() {
    let ready = serde_json::json!({
        "lifecycle":"ready",
        "transport":"rtsp_rtp",
        "codec":"h264",
        "encoder":"nvidia_nvenc",
        "framesObserved":13,
        "lastAccessUnitBytes":4821,
        "lastFrameKeyframe":false
    });
    assert!(sensor_camera_is_started(&ready));

    let no_access_unit = serde_json::json!({
        "lifecycle":"ready",
        "transport":"rtsp_rtp",
        "codec":"h264",
        "encoder":"nvidia_nvenc",
        "framesObserved":13,
        "lastAccessUnitBytes":0,
        "lastFrameKeyframe":false
    });
    assert!(!sensor_camera_is_started(&no_access_unit));
}

#[test]
fn live_preview_orders_reordered_h264_by_decode_sequence() {
    use veoveo_stream_mcp::contract::EncodedVideoChunk;
    let access_unit = BASE64_STANDARD.encode([0, 0, 0, 1, 0x65]);
    let chunks = vec![
        EncodedVideoChunk {
            sequence: 0,
            timestamp_us: 200_000,
            keyframe: true,
            data_base64: access_unit.clone(),
        },
        EncodedVideoChunk {
            sequence: 1,
            timestamp_us: 100_000,
            keyframe: false,
            data_base64: access_unit,
        },
    ];
    validate_live_preview(&chunks).expect("AVC presentation reordering is valid");
    let mut duplicate_time = chunks.clone();
    duplicate_time[1].timestamp_us = duplicate_time[0].timestamp_us;
    assert!(validate_live_preview(&duplicate_time).is_err());
    let mut gap = chunks.clone();
    gap[1].sequence = 2;
    assert!(validate_live_preview(&gap).is_err());
    let mut overflow = chunks;
    overflow[0].sequence = u64::MAX;
    overflow[1].sequence = 0;
    assert!(validate_live_preview(&overflow).is_err());
}

#[test]
fn scenario_ids_use_uav_admission_before_work_starts() {
    let good: Value = serde_json::from_slice(&fs::read(canonical_scenario()).unwrap()).unwrap();
    for field in ["sessionId", "vehicleId"] {
        for invalid in [
            String::new(),
            "unexpected/path".into(),
            "é".into(),
            "x".repeat(129),
        ] {
            let mut wire = good.clone();
            wire[field] = invalid.into();
            assert!(serde_json::from_value::<UavAcceptanceScenario>(wire).is_err());
        }
    }
}

#[test]
fn takeoff_observation_requires_the_actual_accepted_vehicle_acknowledgement() {
    use veoveo_uav_sim_mcp::contract::{CommandAcknowledgement, SessionId, UavResource, VehicleId};
    let session = SessionId::parse("session-alpha").unwrap();
    let vehicle = VehicleId::parse("vehicle-alpha").unwrap();
    let good = CommandAcknowledgement {
        accepted: true,
        detail: "accepted".into(),
        resource_uri: UavResource::Vehicle {
            session: session.clone(),
            vehicle: vehicle.clone(),
        },
    };
    let admit = |value| cleanup::admit_takeoff_acknowledgement(value, &session, &vehicle);
    assert!(admit(serde_json::to_value(&good).unwrap()).is_ok());
    let mut refused = good.clone();
    refused.accepted = false;
    assert!(admit(serde_json::to_value(refused).unwrap()).is_err());
    for resource in [
        UavResource::Vehicle {
            session: SessionId::parse("foreign").unwrap(),
            vehicle: vehicle.clone(),
        },
        UavResource::Vehicle {
            session: session.clone(),
            vehicle: VehicleId::parse("foreign").unwrap(),
        },
        UavResource::Session(session.clone()),
    ] {
        let mut wrong = good.clone();
        wrong.resource_uri = resource;
        assert!(admit(serde_json::to_value(wrong).unwrap()).is_err());
    }
    let mut missing = serde_json::to_value(good).unwrap();
    missing.as_object_mut().unwrap().remove("detail");
    assert!(admit(missing).is_err());
}

#[test]
fn interrupted_flight_stream_uses_registered_domain_cleanup_and_retains_unknown_outcome() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use veoveo_testing_support::lifecycle::owner;
    const MODE: &str = "VEOVEO_TEST_FLIGHT_CLEANUP";
    struct Peer {
        calls: Arc<AtomicUsize>,
        refuse: bool,
    }
    impl cleanup::FlightPeer for Peer {
        async fn call_tool_with_timeout(
            &self,
            tool: &str,
            arguments: Value,
            _: Duration,
        ) -> Result<Value> {
            ensure!(
                owner::check_effect().is_err(),
                "cleanup admitted ordinary effects"
            );
            ensure!(
                tool == "stream__stop_live_session",
                "unexpected cleanup tool"
            );
            self.calls.fetch_add(1, Ordering::SeqCst);
            ensure!(!self.refuse, "fixture remote refusal");
            let session: veoveo_stream_mcp::contract::SessionId =
                serde_json::from_value(arguments["sessionId"].clone())?;
            Ok(serde_json::to_value(StopLiveSessionOutput {
                result_uri: veoveo_stream_mcp::uris::session_uri(session),
                lifecycle: LiveSessionLifecycle::Stopped,
                received_video_frames: 12,
                processed_frames: 12,
                recording_output: None,
                stopped_at: "2026-08-07T18:00:01Z".into(),
            })?)
        }
        async fn resource(&self, uri: &str, _: Duration) -> Result<Value> {
            let value = live_session(1, "pipeline", "stopped");
            ensure!(
                uri == veoveo_stream_mcp::uris::session_uri(value.session_id()).to_string(),
                "cleanup changed parent"
            );
            Ok(serde_json::to_value(value)?)
        }
    }
    if let Ok(mode) = std::env::var(MODE) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let result: Result<()> = runtime.block_on(owner::run(async {
            let owned = cleanup::StreamOwnership::register(
                Peer {
                    calls: calls.clone(),
                    refuse: mode == "refused",
                },
                "https://fixture.invalid/mcp",
                &veoveo_stream_mcp::contract::PipelineId::parse("pipeline").unwrap(),
            )?;
            // Production start records its result before subsequent awaits. The
            // unknown dispatch case intentionally never receives a result.
            if mode != "unknown" {
                owned.observed(live_session(1, "pipeline", "running").session_id())?;
            }
            std::future::pending::<Result<()>>().await
        }));
        assert!(result.is_err(), "interrupted scenario became successful");
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(mode != "unknown"));
        let groups = PathBuf::from(std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
        let unresolved = groups
            .read_dir()
            .unwrap()
            .filter_map(|item| item.ok())
            .any(|entry| entry.file_name().to_string_lossy().contains("unresolved"));
        assert_eq!(
            unresolved,
            mode != "settled",
            "cleanup settlement receipt disagrees"
        );
        return;
    }
    for mode in ["settled", "refused", "unknown"] {
        let root = std::env::temp_dir().join(format!("flight-cleanup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let end = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            + Duration::from_millis(300);
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "domain::tests::interrupted_flight_stream_uses_registered_domain_cleanup_and_retains_unknown_outcome", "--nocapture"])
            .env(MODE, mode).env("VEOVEO_SMOKE_LOCAL_GROUPS", &root)
            .env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", end.as_millis().to_string())
            .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "1")
            .output().unwrap();
        assert!(
            output.status.success(),
            "Flight {mode} control: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn interrupted_takeoff_settles_only_the_selected_vehicle_state() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use veoveo_testing_support::lifecycle::owner;
    const CHILD: &str = "VEOVEO_TEST_FLIGHT_VEHICLE_CLEANUP";
    struct Peer {
        reads: Arc<AtomicUsize>,
    }
    impl cleanup::FlightPeer for Peer {
        async fn call_tool_with_timeout(
            &self,
            tool: &str,
            arguments: Value,
            _: Duration,
        ) -> Result<Value> {
            ensure!(
                owner::check_effect().is_err(),
                "cleanup admitted ordinary effects"
            );
            ensure!(
                tool == "uav-sim__get_simulation_state",
                "landed selected vehicle must not receive another command"
            );
            self.reads.fetch_add(1, Ordering::SeqCst);
            let mut state: veoveo_uav_sim_mcp::contract::SimulationState =
                serde_json::from_str(include_str!("../../tests/fixtures/world-ready.json"))?;
            ensure!(
                arguments["sessionId"] == state.session_id.to_string(),
                "cleanup changed selected session"
            );
            // Another aircraft is still flying. It must not be landed by this
            // invocation, nor prevent its selected landed aircraft settling.
            let mut foreign = state.vehicles[0].clone();
            foreign.vehicle_id =
                veoveo_uav_sim_mcp::contract::VehicleId::parse("foreign-aircraft")?;
            foreign.flight_state = veoveo_uav_sim_mcp::contract::VehicleFlightState::Flying;
            state.vehicles.insert(0, foreign);
            Ok(serde_json::to_value(state)?)
        }
        async fn resource(&self, _: &str, _: Duration) -> Result<Value> {
            bail!("unexpected vehicle resource")
        }
    }
    if std::env::var_os(CHILD).is_some() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let reads = Arc::new(AtomicUsize::new(0));
        let result: Result<()> = runtime.block_on(owner::run(async {
            let scenario = UavAcceptanceScenario::load(&canonical_scenario())?;
            let _registered = cleanup::register_vehicle(
                Peer {
                    reads: reads.clone(),
                },
                "https://fixture.invalid/mcp",
                &scenario,
            )?;
            // Interrupt after intent, before any acknowledgement: authoritative
            // selected-parent state can settle landing without a repeated effect.
            std::future::pending::<Result<()>>().await
        }));
        assert!(result.is_err());
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        let root = PathBuf::from(std::env::var_os("VEOVEO_SMOKE_LOCAL_GROUPS").unwrap());
        assert!(
            !root
                .read_dir()
                .unwrap()
                .filter_map(|item| item.ok())
                .any(|entry| entry.file_name().to_string_lossy().contains("unresolved"))
        );
        return;
    }
    let root = std::env::temp_dir().join(format!("flight-vehicle-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let end = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        + Duration::from_millis(300);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "domain::tests::interrupted_takeoff_settles_only_the_selected_vehicle_state",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .env("VEOVEO_SMOKE_LOCAL_GROUPS", &root)
        .env("VEOVEO_SMOKE_DEADLINE_UNIX_MS", end.as_millis().to_string())
        .env("VEOVEO_SMOKE_CLEANUP_SECONDS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "vehicle cleanup control: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn current_scenario_refuses_retired_root_and_nested_members() {
    let current: Value = serde_json::from_slice(&fs::read(canonical_scenario()).unwrap()).unwrap();
    let admitted: UavAcceptanceScenario = serde_json::from_value(current.clone()).unwrap();
    admitted.validate().unwrap();
    for (parent, canonical, retired) in [
        ("", "sessionId", "session_id"),
        ("", "mapMobilityProfileUri", "map_mobility_profile_uri"),
        ("/world", "simulationFrameId", "simulation_frame_id"),
        ("/takeoff", "relativeAltitudeM", "relative_altitude_m"),
        ("/camera", "streamTimeoutSeconds", "stream_timeout_seconds"),
        ("/mission", "taskTimeoutSeconds", "task_timeout_seconds"),
        (
            "/stream/recordingReplay",
            "rangeLagSeconds",
            "range_lag_seconds",
        ),
        ("/reason", "maximumFrames", "maximum_frames"),
        (
            "/view",
            "minimumMissionSensorFrames",
            "minimum_mission_sensor_frames",
        ),
    ] {
        for mixed in [false, true] {
            let mut bad = current.clone();
            let fields = bad.pointer_mut(parent).unwrap().as_object_mut().unwrap();
            let value = fields[canonical].clone();
            if !mixed {
                fields.remove(canonical);
            }
            fields.insert(retired.into(), value);
            assert!(
                serde_json::from_value::<UavAcceptanceScenario>(bad).is_err(),
                "{parent}/{canonical} mixed={mixed}"
            );
        }
    }
}
