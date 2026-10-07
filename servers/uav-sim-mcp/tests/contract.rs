fn capture_schema<T: schemars::JsonSchema>(
    schemas: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
) {
    schemas.insert(
        name.into(),
        serde_json::to_value(schemars::schema_for!(T)).unwrap(),
    );
}
#[path = "contract/recordings.rs"]
mod recordings;
#[path = "contract/resources.rs"]
mod resources;

use veoveo_uav_sim_mcp::contract::*;

#[test]
fn scopes_admit_only_the_domain_vocabulary_and_preserve_wire_names() {
    let expected = [
        (UavScope::Read, "uav-sim:read"),
        (UavScope::Control, "uav-sim:control"),
        (UavScope::Admin, "uav-sim:admin"),
        (UavScope::Stream, "uav-sim:stream"),
    ];
    assert_eq!(UavScope::ALL.len(), expected.len());
    for (scope, wire) in expected {
        assert_eq!(wire.parse::<UavScope>().unwrap(), scope);
        assert_eq!(serde_json::to_value(scope).unwrap(), wire);
        assert_eq!(
            serde_json::from_value::<UavScope>(wire.into()).unwrap(),
            scope
        );
        assert_eq!(scope.to_string(), wire);
    }
    for unknown in [
        "uav-sim:unknown",
        "other:read",
        "uav-sim:READ",
        "uav-sim:read uav-sim:admin",
        "",
    ] {
        assert!(unknown.parse::<UavScope>().is_err());
        assert!(serde_json::from_value::<UavScope>(unknown.into()).is_err());
    }
    let schema = serde_json::to_value(schemars::schema_for!(UavScope)).unwrap();
    assert_eq!(
        schema["enum"],
        serde_json::json!(expected.map(|(_, name)| name))
    );
}

#[test]
fn schemas_preserve_the_published_contract() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/contract.schema.json");
    let mut schemas = serde_json::Map::new();

    capture_schema::<ActiveVehicleGrantsRequest>(
        &mut schemas,
        stringify!(ActiveVehicleGrantsRequest),
    );
    capture_schema::<CameraCodec>(&mut schemas, stringify!(CameraCodec));
    capture_schema::<CameraEncoder>(&mut schemas, stringify!(CameraEncoder));
    capture_schema::<CameraLifecycle>(&mut schemas, stringify!(CameraLifecycle));
    capture_schema::<CameraRenderPoseState>(&mut schemas, stringify!(CameraRenderPoseState));
    capture_schema::<CameraState>(&mut schemas, stringify!(CameraState));
    capture_schema::<CameraTransport>(&mut schemas, stringify!(CameraTransport));
    capture_schema::<CaptureDatasetRequest>(&mut schemas, stringify!(CaptureDatasetRequest));
    capture_schema::<CaptureDatasetResult>(&mut schemas, stringify!(CaptureDatasetResult));
    capture_schema::<CloseLiveViewRequest>(&mut schemas, stringify!(CloseLiveViewRequest));
    capture_schema::<CloseLiveViewResult>(&mut schemas, stringify!(CloseLiveViewResult));
    capture_schema::<CollectionPage<VehicleMissionPlan>>(
        &mut schemas,
        stringify!(CollectionPage<VehicleMissionPlan>),
    );
    capture_schema::<CommandAcknowledgement>(&mut schemas, stringify!(CommandAcknowledgement));
    capture_schema::<ConfigureWorldOutput>(&mut schemas, stringify!(ConfigureWorldOutput));
    capture_schema::<ConfigureWorldRequest>(&mut schemas, stringify!(ConfigureWorldRequest));
    capture_schema::<ControlGrantId>(&mut schemas, stringify!(ControlGrantId));
    capture_schema::<DurableOperation>(&mut schemas, stringify!(DurableOperation));
    capture_schema::<DurableOperationResult>(&mut schemas, stringify!(DurableOperationResult));
    capture_schema::<EnuDirection>(&mut schemas, stringify!(EnuDirection));
    capture_schema::<EnuVector>(&mut schemas, stringify!(EnuVector));
    capture_schema::<ExecuteMissionRequest>(&mut schemas, stringify!(ExecuteMissionRequest));
    capture_schema::<ExecuteVehicleMissionPlanRequest>(
        &mut schemas,
        stringify!(ExecuteVehicleMissionPlanRequest),
    );
    capture_schema::<GrantVehicleControlRequest>(
        &mut schemas,
        stringify!(GrantVehicleControlRequest),
    );
    capture_schema::<MissionId>(&mut schemas, stringify!(MissionId));
    capture_schema::<MissionLifecycle>(&mut schemas, stringify!(MissionLifecycle));
    capture_schema::<MissionPlanId>(&mut schemas, stringify!(MissionPlanId));
    capture_schema::<MissionPlanLifecycle>(&mut schemas, stringify!(MissionPlanLifecycle));
    capture_schema::<MissionResult>(&mut schemas, stringify!(MissionResult));
    capture_schema::<MissionWaypoint>(&mut schemas, stringify!(MissionWaypoint));
    capture_schema::<NedVector>(&mut schemas, stringify!(NedVector));
    capture_schema::<OpenLiveViewRequest>(&mut schemas, stringify!(OpenLiveViewRequest));
    capture_schema::<PrepareVehicleMissionRequest>(
        &mut schemas,
        stringify!(PrepareVehicleMissionRequest),
    );
    capture_schema::<QuaternionXyzw>(&mut schemas, stringify!(QuaternionXyzw));
    capture_schema::<RecordingCatalogLifecycle>(
        &mut schemas,
        stringify!(RecordingCatalogLifecycle),
    );
    capture_schema::<RecordingKey>(&mut schemas, stringify!(RecordingKey));
    capture_schema::<RecordingPublisherLifecycle>(
        &mut schemas,
        stringify!(RecordingPublisherLifecycle),
    );
    capture_schema::<RecordingState>(&mut schemas, stringify!(RecordingState));
    capture_schema::<RenewLiveViewRequest>(&mut schemas, stringify!(RenewLiveViewRequest));
    capture_schema::<RevokeVehicleControlRequest>(
        &mut schemas,
        stringify!(RevokeVehicleControlRequest),
    );
    capture_schema::<RunScenarioRequest>(&mut schemas, stringify!(RunScenarioRequest));
    capture_schema::<RuntimeTimingState>(&mut schemas, stringify!(RuntimeTimingState));
    capture_schema::<ScenarioResult>(&mut schemas, stringify!(ScenarioResult));
    capture_schema::<SessionId>(&mut schemas, stringify!(SessionId));
    capture_schema::<SessionRequest>(&mut schemas, stringify!(SessionRequest));
    capture_schema::<SimulationCommand>(&mut schemas, stringify!(SimulationCommand));
    capture_schema::<SimulationLifecycle>(&mut schemas, stringify!(SimulationLifecycle));
    capture_schema::<SimulationState>(&mut schemas, stringify!(SimulationState));
    capture_schema::<SimulationWorldBinding>(&mut schemas, stringify!(SimulationWorldBinding));
    capture_schema::<StepSimulationRequest>(&mut schemas, stringify!(StepSimulationRequest));
    capture_schema::<TakeoffRequest>(&mut schemas, stringify!(TakeoffRequest));
    capture_schema::<TileFailureCode>(&mut schemas, stringify!(TileFailureCode));
    capture_schema::<TileFailureState>(&mut schemas, stringify!(TileFailureState));
    capture_schema::<TileLifecycle>(&mut schemas, stringify!(TileLifecycle));
    capture_schema::<TileLoadType>(&mut schemas, stringify!(TileLoadType));
    capture_schema::<TileState>(&mut schemas, stringify!(TileState));
    capture_schema::<VehicleControlGrant>(&mut schemas, stringify!(VehicleControlGrant));
    capture_schema::<VehicleControlPermission>(&mut schemas, stringify!(VehicleControlPermission));
    capture_schema::<VehicleFlightState>(&mut schemas, stringify!(VehicleFlightState));
    capture_schema::<VehicleId>(&mut schemas, stringify!(VehicleId));
    capture_schema::<VehicleMission>(&mut schemas, stringify!(VehicleMission));
    capture_schema::<VehicleMissionPlan>(&mut schemas, stringify!(VehicleMissionPlan));
    capture_schema::<VehicleRequest>(&mut schemas, stringify!(VehicleRequest));
    capture_schema::<VehicleState>(&mut schemas, stringify!(VehicleState));
    capture_schema::<LiveCameraDescriptor>(&mut schemas, stringify!(LiveCameraDescriptor));
    capture_schema::<LiveCameraHealth>(&mut schemas, stringify!(LiveCameraHealth));
    capture_schema::<LiveCameraId>(&mut schemas, stringify!(LiveCameraId));
    capture_schema::<LiveCameraRegion>(&mut schemas, stringify!(LiveCameraRegion));
    capture_schema::<LiveCameraRig>(&mut schemas, stringify!(LiveCameraRig));
    capture_schema::<LiveCameraSmoothing>(&mut schemas, stringify!(LiveCameraSmoothing));
    capture_schema::<LiveCameraSource>(&mut schemas, stringify!(LiveCameraSource));
    capture_schema::<LiveCameraStreamPolicy>(&mut schemas, stringify!(LiveCameraStreamPolicy));
    capture_schema::<LiveColorMatrix>(&mut schemas, stringify!(LiveColorMatrix));
    capture_schema::<LiveColorMetadata>(&mut schemas, stringify!(LiveColorMetadata));
    capture_schema::<LiveColorPrimaries>(&mut schemas, stringify!(LiveColorPrimaries));
    capture_schema::<LiveColorRange>(&mut schemas, stringify!(LiveColorRange));
    capture_schema::<LiveColorTransfer>(&mut schemas, stringify!(LiveColorTransfer));
    capture_schema::<LiveEntityId>(&mut schemas, stringify!(LiveEntityId));
    capture_schema::<LiveMediaEndpoint>(&mut schemas, stringify!(LiveMediaEndpoint));
    capture_schema::<LiveMediaTransport>(&mut schemas, stringify!(LiveMediaTransport));
    capture_schema::<LivePose>(&mut schemas, stringify!(LivePose));
    capture_schema::<LiveQuaternionXyzw>(&mut schemas, stringify!(LiveQuaternionXyzw));
    capture_schema::<LiveSessionId>(&mut schemas, stringify!(LiveSessionId));
    capture_schema::<LiveStreamProductId>(&mut schemas, stringify!(LiveStreamProductId));
    capture_schema::<LiveStreamProductLifecycle>(
        &mut schemas,
        stringify!(LiveStreamProductLifecycle),
    );
    capture_schema::<LiveStreamProductState>(&mut schemas, stringify!(LiveStreamProductState));
    capture_schema::<LiveVector3>(&mut schemas, stringify!(LiveVector3));
    capture_schema::<LiveViewAccessToken>(&mut schemas, stringify!(LiveViewAccessToken));
    capture_schema::<LiveViewCodec>(&mut schemas, stringify!(LiveViewCodec));
    capture_schema::<LiveViewConnection>(&mut schemas, stringify!(LiveViewConnection));
    capture_schema::<LiveViewHardwareEncoder>(&mut schemas, stringify!(LiveViewHardwareEncoder));
    capture_schema::<LiveViewId>(&mut schemas, stringify!(LiveViewId));
    capture_schema::<LiveViewLifecycle>(&mut schemas, stringify!(LiveViewLifecycle));
    capture_schema::<LiveViewOwner>(&mut schemas, stringify!(LiveViewOwner));
    capture_schema::<LiveViewState>(&mut schemas, stringify!(LiveViewState));
    capture_schema::<LiveViewUri>(&mut schemas, stringify!(LiveViewUri));
    capture_schema::<LiveViewerInstanceId>(&mut schemas, stringify!(LiveViewerInstanceId));
    if std::env::var_os("VEOVEO_UPDATE_UAV_CONTRACT_SCHEMA").is_some() {
        std::fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(&schemas).unwrap()),
        )
        .unwrap();
    }
    let baseline: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(schemas, baseline);
}

#[test]
fn public_ids_and_commands_admit_the_existing_wire_profile() {
    for value in ["alpha", "Upper_123", "with.dot", "with-dash"] {
        let request: SessionRequest =
            serde_json::from_value(serde_json::json!({"sessionId":value})).unwrap();
        assert_eq!(request.session_id.as_str(), value);
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({"sessionId":value})
        );
    }
    for value in [String::new(), "a/b".into(), "é".into(), "x".repeat(129)] {
        assert!(
            serde_json::from_value::<SessionRequest>(serde_json::json!({"sessionId":value}))
                .is_err()
        );
        assert!(serde_json::from_value::<VehicleId>(serde_json::json!(value)).is_err());
    }
    let operation: DurableOperation = serde_json::from_value(serde_json::json!({
        "operation":"capture_dataset", "input":{"sessionId":"alpha", "durationSeconds":2.0, "sensors":["down-camera"]}
    })).unwrap();
    assert_eq!(operation.task_type().as_str(), "capture_dataset");
}

#[test]
fn contract_consumers_validate_live_product_geometry_and_secrets() {
    let mut product = LiveStreamProductState {
        stream_product_id: LiveStreamProductId::parse("atlas").unwrap(),
        camera_regions: vec![LiveCameraRegion {
            camera_id: LiveCameraId::parse("follow").unwrap(),
            x_px: 0,
            y_px: 0,
            width_px: 640,
            height_px: 480,
        }],
        coded_width_px: 640,
        coded_height_px: 480,
        lifecycle: LiveStreamProductLifecycle::Ready,
        active_viewers: 0,
        connected_viewers: 0,
        nvenc_sessions: 1,
        encoded_frames: 1,
        source_to_render_p95_microseconds: None,
        source_to_render_samples: 0,
        last_frame_at: None,
        visible: None,
        diagnostic: None,
    };
    assert!(product.validate());
    let encoded = serde_json::to_value(&product).unwrap();
    assert_eq!(
        serde_json::from_value::<LiveStreamProductState>(encoded).unwrap(),
        product
    );
    product.camera_regions[0].x_px = 1;
    assert!(!product.validate());
    let token = LiveViewAccessToken::parse("a".repeat(32)).unwrap();
    assert_eq!(format!("{token:?}"), "LiveViewAccessToken(<redacted>)");
    assert_eq!(token.expose_for_stream(), "a".repeat(32));
}

#[path = "../../../testing/fixtures/tool_inputs.rs"]
mod input_fixture;

#[test]
fn current_tool_roots_refuse_replacement_and_mixed_retired_members() {
    fn admitted(tool: &str, value: serde_json::Value) -> bool {
        match tool {
            "grant_vehicle_control" => {
                serde_json::from_value::<GrantVehicleControlRequest>(value).is_ok()
            }
            "get_simulation_state" => serde_json::from_value::<SessionRequest>(value).is_ok(),
            "configure_world" => serde_json::from_value::<ConfigureWorldRequest>(value).is_ok(),
            "prepare_vehicle_mission" => {
                serde_json::from_value::<PrepareVehicleMissionRequest>(value).is_ok()
            }
            _ => panic!("unexpected UAV controlled-input tool"),
        }
    }
    let cases =
        input_fixture::ToolInputCase::load(include_bytes!("../testdata/controlled-inputs.json"));
    assert_eq!(cases.len(), 23);
    let mut controls = 0;
    for case in cases {
        assert!(
            admitted(&case.tool, case.arguments.clone()),
            "{} current input",
            case.branch
        );
        for (name, value) in case.arguments.as_object().unwrap() {
            let retired: String = name
                .chars()
                .flat_map(|c| {
                    if c.is_ascii_uppercase() {
                        vec!['_', c.to_ascii_lowercase()]
                    } else {
                        vec![c]
                    }
                })
                .collect();
            if retired == *name {
                continue;
            }
            for mixed in [false, true] {
                let mut bad = case.arguments.clone();
                let fields = bad.as_object_mut().unwrap();
                if !mixed {
                    fields.remove(name);
                }
                fields.insert(retired.clone(), value.clone());
                assert!(
                    !admitted(&case.tool, bad),
                    "{} {name} mixed={mixed}",
                    case.branch
                );
                controls += 1;
            }
        }
    }
    assert!(controls > 40);
}

#[test]
fn private_nested_current_wire_refuses_retired_and_mixed_members() {
    fn check<T: serde::Serialize + serde::de::DeserializeOwned>(
        value: T,
        members: &[(&str, &str)],
    ) {
        let current = serde_json::to_value(value).unwrap();
        assert!(serde_json::from_value::<T>(current.clone()).is_ok());
        for (canonical, retired) in members {
            assert!(current.get(*canonical).is_some());
            assert!(current.get(*retired).is_none());
            for mixed in [false, true] {
                let mut bad = current.clone();
                let fields = bad.as_object_mut().unwrap();
                let value = fields.get(*canonical).unwrap().clone();
                if !mixed {
                    fields.remove(*canonical);
                }
                fields.insert((*retired).into(), value);
                assert!(
                    serde_json::from_value::<T>(bad.clone()).is_err(),
                    "{canonical} mixed={mixed}"
                );
                assert!(
                    serde_json::from_str::<T>(&bad.to_string()).is_err(),
                    "JSON {canonical} mixed={mixed}"
                );
            }
        }
    }
    check(
        MissionWaypoint {
            position: Wgs84Position {
                latitude_degrees: 0.0,
                longitude_degrees: 0.0,
                ellipsoid_height_m: 0.0,
            },
            speed_mps: 1.0,
            hold_seconds: 0.0,
        },
        &[("speedMps", "speed_mps"), ("holdSeconds", "hold_seconds")],
    );
    check(
        EnuVector {
            east_m: 1.0,
            north_m: 2.0,
            up_m: 3.0,
        },
        &[("eastM", "east_m"), ("northM", "north_m"), ("upM", "up_m")],
    );
    check(
        TileFailureState {
            code: TileFailureCode::TransportFailed,
            load_type: TileLoadType::TileContent,
            http_status: 503,
            generation: 1,
        },
        &[("loadType", "load_type"), ("httpStatus", "http_status")],
    );
}
