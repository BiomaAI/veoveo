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
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/contract.schema.json")).unwrap();
    macro_rules! check { ($($ty:ty),+ $(,)?) => { $(assert_eq!(serde_json::to_value(schemars::schema_for!($ty)).unwrap(), baseline[stringify!($ty)], stringify!($ty));)+ }; }
    check!(
        ActiveVehicleGrantsRequest,
        CameraCodec,
        CameraEncoder,
        CameraLifecycle,
        CameraRenderPoseState,
        CameraState,
        CameraTransport,
        CaptureDatasetRequest,
        CaptureDatasetResult,
        CloseLiveViewRequest,
        CloseLiveViewResult,
        CollectionPage<VehicleMissionPlan>,
        CommandAcknowledgement,
        ConfigureWorldOutput,
        ConfigureWorldRequest,
        ControlGrantId,
        DurableOperation,
        DurableOperationResult,
        EnuDirection,
        EnuVector,
        ExecuteMissionRequest,
        ExecuteVehicleMissionPlanRequest,
        GrantVehicleControlRequest,
        MissionId,
        MissionLifecycle,
        MissionPlanId,
        MissionPlanLifecycle,
        MissionResult,
        MissionWaypoint,
        NedVector,
        OpenLiveViewRequest,
        PrepareVehicleMissionRequest,
        QuaternionXyzw,
        RecordingCatalogLifecycle,
        RecordingId,
        RecordingKey,
        RecordingPublisherLifecycle,
        RecordingState,
        RenewLiveViewRequest,
        RevokeVehicleControlRequest,
        RunScenarioRequest,
        RuntimeTimingState,
        ScenarioResult,
        SessionId,
        SessionRequest,
        SimulationCommand,
        SimulationLifecycle,
        SimulationState,
        SimulationWorldBinding,
        StepSimulationRequest,
        TakeoffRequest,
        TileFailureCode,
        TileFailureState,
        TileLifecycle,
        TileLoadType,
        TileState,
        VehicleControlGrant,
        VehicleControlPermission,
        VehicleFlightState,
        VehicleId,
        VehicleMission,
        VehicleMissionPlan,
        VehicleRequest,
        VehicleState,
        LiveCameraDescriptor,
        LiveCameraHealth,
        LiveCameraId,
        LiveCameraRegion,
        LiveCameraRig,
        LiveCameraSmoothing,
        LiveCameraSource,
        LiveCameraStreamPolicy,
        LiveColorMatrix,
        LiveColorMetadata,
        LiveColorPrimaries,
        LiveColorRange,
        LiveColorTransfer,
        LiveEntityId,
        LiveMediaEndpoint,
        LiveMediaTransport,
        LivePose,
        LiveQuaternionXyzw,
        LiveSessionId,
        LiveStreamProductId,
        LiveStreamProductLifecycle,
        LiveStreamProductState,
        LiveVector3,
        LiveViewAccessToken,
        LiveViewCodec,
        LiveViewConnection,
        LiveViewHardwareEncoder,
        LiveViewId,
        LiveViewLifecycle,
        LiveViewOwner,
        LiveViewState,
        LiveViewUri,
        LiveViewerInstanceId,
    );
}

#[test]
fn public_ids_and_commands_admit_the_existing_wire_profile() {
    for value in ["alpha", "Upper_123", "with.dot", "with-dash"] {
        let request: SessionRequest =
            serde_json::from_value(serde_json::json!({"session_id":value})).unwrap();
        assert_eq!(request.session_id.as_str(), value);
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({"session_id":value})
        );
    }
    for value in [String::new(), "a/b".into(), "é".into(), "x".repeat(129)] {
        assert!(
            serde_json::from_value::<SessionRequest>(serde_json::json!({"session_id":value}))
                .is_err()
        );
        assert!(serde_json::from_value::<VehicleId>(serde_json::json!(value)).is_err());
    }
    let operation: DurableOperation = serde_json::from_value(serde_json::json!({
        "operation":"capture_dataset", "input":{"session_id":"alpha", "duration_seconds":2.0, "sensors":["down-camera"]}
    })).unwrap();
    assert_eq!(operation.task_type().as_str(), "capture_dataset");
}

#[test]
fn contract_consumers_validate_live_product_geometry_and_secrets() {
    let mut product = LiveStreamProductState {
        stream_product_id: LiveStreamProductId::new("atlas").unwrap(),
        camera_regions: vec![LiveCameraRegion {
            camera_id: LiveCameraId::new("follow").unwrap(),
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
    let token = LiveViewAccessToken::new("a".repeat(32)).unwrap();
    assert_eq!(format!("{token:?}"), "LiveViewAccessToken(<redacted>)");
    assert_eq!(token.expose_for_stream(), "a".repeat(32));
}
