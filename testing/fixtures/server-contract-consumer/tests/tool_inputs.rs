//! Input qualification through the same public owner types used by tool extraction.
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap()
}

fn root_is_closed(value: &Value) -> bool {
    if value.get("additionalProperties") == Some(&Value::Bool(false)) {
        return true;
    }
    for keyword in ["oneOf", "anyOf"] {
        if let Some(branches) = value.get(keyword).and_then(Value::as_array) {
            return !branches.is_empty() && branches.iter().all(root_is_closed);
        }
    }
    value
        .get("allOf")
        .and_then(Value::as_array)
        .is_some_and(|branches| branches.iter().any(root_is_closed))
}

fn qualify_schema<T: JsonSchema>() {
    let value = schema::<T>();
    jsonschema::meta::validate(&value).unwrap();
    assert!(
        root_is_closed(&value),
        "{} has an open root: {value}",
        std::any::type_name::<T>()
    );
}

fn rejects_extra<T: JsonSchema + DeserializeOwned>(value: Value, pointer: &str) {
    let validator = jsonschema::validator_for(&schema::<T>()).unwrap();
    assert!(
        validator.is_valid(&value),
        "{} baseline schema: {value}",
        std::any::type_name::<T>()
    );
    assert!(
        serde_json::from_value::<T>(value.clone()).is_ok(),
        "{} baseline decoder: {value}",
        std::any::type_name::<T>()
    );
    let mut extra = value;
    extra
        .pointer_mut(pointer)
        .unwrap()
        .as_object_mut()
        .unwrap()
        .insert("unexpected_field".into(), json!(true));
    assert!(
        !validator.is_valid(&extra),
        "{} schema accepted extra at {pointer}: {extra}",
        std::any::type_name::<T>()
    );
    assert!(
        serde_json::from_value::<T>(extra).is_err(),
        "{} decoder accepted extra at {pointer}",
        std::any::type_name::<T>()
    );
}

#[test]
fn every_production_tool_root_publishes_a_closed_owner_schema() {
    qualify_schema::<veoveo_artifact_mcp::contract::ArtifactReference>();
    qualify_schema::<veoveo_artifact_mcp::contract::CreateArtifactShareRequest>();
    qualify_schema::<veoveo_artifact_mcp::contract::GrantArtifactRequest>();
    qualify_schema::<veoveo_artifact_mcp::contract::RevokeArtifactGrantRequest>();
    qualify_schema::<veoveo_artifact_mcp::contract::RevokeArtifactShareRequest>();
    qualify_schema::<veoveo_artifact_mcp::contract::SetArtifactReleaseRequest>();
    qualify_schema::<veoveo_computers_mcp::contract::CreateInput>();
    qualify_schema::<veoveo_computers_mcp::contract::ExecuteInput>();
    qualify_schema::<veoveo_computers_mcp::contract::IssueAutomationGrantInput>();
    qualify_schema::<veoveo_computers_mcp::contract::LifecycleInput>();
    qualify_schema::<veoveo_computers_mcp::contract::ResumeUpdateInput>();
    qualify_schema::<veoveo_computers_mcp::contract::RevokeAccessInput>();
    qualify_schema::<veoveo_computers_mcp::contract::RevokeAutomationGrantInput>();
    qualify_schema::<veoveo_computers_mcp::contract::TransferFileInput>();
    qualify_schema::<veoveo_computers_mcp::contract::UpdateTemplateInput>();
    qualify_schema::<veoveo_duckdb_mcp::contract::DuckDbExecuteRequest>();
    qualify_schema::<veoveo_duckdb_mcp::contract::DuckDbExportRequest>();
    qualify_schema::<veoveo_duckdb_mcp::contract::DuckDbIngestRequest>();
    qualify_schema::<veoveo_duckdb_mcp::contract::DuckDbQueryRequest>();
    qualify_schema::<veoveo_frames_mcp::contract::BatchTransformRequest>();
    qualify_schema::<veoveo_frames_mcp::contract::ConvertFrameRequest>();
    qualify_schema::<veoveo_frames_mcp::contract::CreateWorldRequest>();
    qualify_schema::<veoveo_frames_mcp::contract::PublishWorldRequest>();
    qualify_schema::<veoveo_knowledge_mcp::contract::EmbedRequest>();
    qualify_schema::<veoveo_knowledge_mcp::contract::SearchRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ArchiveFeatureLayerRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ArchiveMapCompositionRequest>();
    qualify_schema::<veoveo_map_mcp::contract::BuildTravelModelRequest>();
    qualify_schema::<veoveo_map_mcp::contract::BuildVectorTilesRequest>();
    qualify_schema::<veoveo_map_mcp::contract::CancelAcquisitionRequest>();
    qualify_schema::<veoveo_map_mcp::contract::CommitFeatureChangesRequest>();
    qualify_schema::<veoveo_map_mcp::contract::CorridorInspectionRequest>();
    qualify_schema::<veoveo_map_mcp::contract::CreateAcquisitionRequest>();
    qualify_schema::<veoveo_map_mcp::contract::CreateFeatureLayerRequest>();
    qualify_schema::<veoveo_map_mcp::contract::CreateMapCompositionRequest>();
    qualify_schema::<veoveo_map_mcp::contract::CreateMobilityProfileRequest>();
    qualify_schema::<veoveo_map_mcp::contract::CreateSourceRequest>();
    qualify_schema::<veoveo_map_mcp::contract::DeriveRasterRequest>();
    qualify_schema::<veoveo_map_mcp::contract::DeriveSpatialGeometryRequest>();
    qualify_schema::<veoveo_map_mcp::contract::DisableSourceRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ExportFeatureLayerRequest>();
    qualify_schema::<veoveo_map_mcp::contract::GeodesicDirectRequest>();
    qualify_schema::<veoveo_map_mcp::contract::GeodesicInverseRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ImportFeatureLayerRequest>();
    qualify_schema::<veoveo_map_mcp::contract::InspectGeoPackageRequest>();
    qualify_schema::<veoveo_map_mcp::contract::InspectLocationRequest>();
    qualify_schema::<veoveo_map_mcp::contract::InspectPositionRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ListActiveDatasetReleasesRequest>();
    qualify_schema::<veoveo_map_mcp::contract::PrepareRouteHandoffRequest>();
    qualify_schema::<veoveo_map_mcp::contract::PublishFeatureLayerRequest>();
    qualify_schema::<veoveo_map_mcp::contract::PublishRestrictionRequest>();
    qualify_schema::<veoveo_map_mcp::contract::QueryFeaturesRequest>();
    qualify_schema::<veoveo_map_mcp::contract::QuerySourceFeaturesRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ReachableAreaRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ReleaseMutationRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ReplaceSourceRequest>();
    qualify_schema::<veoveo_map_mcp::contract::RestoreFeatureRequest>();
    qualify_schema::<veoveo_map_mcp::contract::RouteMatrixRequest>();
    qualify_schema::<veoveo_map_mcp::contract::RouteRequest>();
    qualify_schema::<veoveo_map_mcp::contract::SearchLocationsRequest>();
    qualify_schema::<veoveo_map_mcp::contract::TransformCrsRequest>();
    qualify_schema::<veoveo_map_mcp::contract::UpdateFeatureLayerRequest>();
    qualify_schema::<veoveo_map_mcp::contract::UpdateMapCompositionRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ValidateFeatureChangesRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ValidateGeofenceRequest>();
    qualify_schema::<veoveo_map_mcp::contract::ValidateRouteRequest>();
    qualify_schema::<veoveo_map_mcp::contract::WithdrawRestrictionRequest>();
    qualify_schema::<veoveo_media_mcp::contract::ArtifactArgs>();
    qualify_schema::<veoveo_media_mcp::contract::ModelSchemaArgs>();
    qualify_schema::<veoveo_media_mcp::contract::ModelsArgs>();
    qualify_schema::<veoveo_media_mcp::contract::RunArgs>();
    qualify_schema::<veoveo_optimization_mcp::contract::OptimizeRouteScenariosRequest>();
    qualify_schema::<veoveo_optimization_mcp::contract::OptimizeRoutesRequest>();
    qualify_schema::<veoveo_optimization_mcp::contract::SolveConvexRequest>();
    qualify_schema::<veoveo_optimization_mcp::contract::SolveMilpRequest>();
    qualify_schema::<veoveo_optimization_mcp::contract::VerifySolutionRequest>();
    qualify_schema::<veoveo_reason_mcp::contract::AnalyzeRecordingRequest>();
    qualify_schema::<veoveo_recording_mcp::contract::CreateRecordingProjectionRequest>();
    qualify_schema::<veoveo_recording_mcp::contract::SealRecordingRequest>();
    qualify_schema::<veoveo_speech_mcp::contract::dictation::DictationId>();
    qualify_schema::<veoveo_speech_mcp::contract::dictation::StartDictation>();
    qualify_schema::<veoveo_speech_mcp::contract::TranscribeRequest>();
    qualify_schema::<veoveo_stream_mcp::contract::RunRecordingRequest>();
    qualify_schema::<veoveo_stream_mcp::contract::StartLiveSessionRequest>();
    qualify_schema::<veoveo_stream_mcp::contract::StopLiveSessionRequest>();
    qualify_schema::<veoveo_time_mcp::contract::AssessClockRequest>();
    qualify_schema::<veoveo_time_mcp::contract::CancelTemporalEventRequest>();
    qualify_schema::<veoveo_time_mcp::contract::ConvertTimeRequest>();
    qualify_schema::<veoveo_time_mcp::contract::CreateTemporalEventRequest>();
    qualify_schema::<veoveo_time_mcp::contract::EvaluateWindowsRequest>();
    qualify_schema::<veoveo_time_mcp::contract::ExpandScheduleRequest>();
    qualify_schema::<veoveo_time_mcp::contract::ResolveTimeRequest>();
    qualify_schema::<veoveo_time_mcp::contract::ValidateTimelineRequest>();
    qualify_schema::<veoveo_timeseries_mcp::contract::TimeseriesForecastRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::ActiveVehicleGrantsRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::CaptureDatasetRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::CloseLiveViewRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::ConfigureWorldRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::ExecuteVehicleMissionPlanRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::GrantVehicleControlRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::OpenLiveViewRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::PrepareVehicleMissionRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::RenewLiveViewRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::RevokeVehicleControlRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::RunScenarioRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::SessionRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::StepSimulationRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::TakeoffRequest>();
    qualify_schema::<veoveo_uav_sim_mcp::contract::VehicleRequest>();
    qualify_schema::<veoveo_view_mcp::contract::CaptureFrameRequest>();
    qualify_schema::<veoveo_view_mcp::contract::CloseViewRequest>();
    qualify_schema::<veoveo_view_mcp::contract::CreateSceneCompositionRequest>();
    qualify_schema::<veoveo_view_mcp::contract::CreateViewRequest>();
    qualify_schema::<veoveo_view_mcp::contract::SetCameraRequest>();
}

#[test]
fn valid_owner_requests_reject_extra_keys_in_both_schema_and_decoder() {
    use veoveo_artifact_mcp::contract::{ArtifactId, ArtifactReference};
    let artifact = ArtifactId::new();
    rejects_extra::<ArtifactReference>(
        serde_json::to_value(ArtifactReference {
            artifact_id: artifact,
        })
        .unwrap(),
        "",
    );
    rejects_extra::<veoveo_speech_mcp::contract::TranscribeRequest>(
        json!({"artifact_uri":artifact.plane_uri()}),
        "",
    );
    rejects_extra::<veoveo_media_mcp::contract::ModelsArgs>(json!({}), "");
    // Arbitrary provider input stays accepted even when it resembles a schema.
    rejects_extra::<veoveo_media_mcp::contract::RunArgs>(
        json!({"model":"openai/gpt-image-2/edit", "input":{
            "custom_provider_option":{"$ref":"https://example.invalid/provider", "arbitrary":true}
        }}),
        "",
    );
    rejects_extra::<veoveo_frames_mcp::contract::CreateWorldRequest>(
        json!({"world_id":"survey", "display_name":"Survey"}),
        "",
    );
    use veoveo_frames_mcp::contract::{
        BatchTransformRequest, ConvertFrameRequest, CoordinatePoint, CoordinateSpace,
        Wgs84Position as FrameWgs84Position,
    };
    let batch = serde_json::to_value(BatchTransformRequest {
        convert: ConvertFrameRequest {
            target: CoordinateSpace::Wgs84,
            points: vec![CoordinatePoint::Wgs84(FrameWgs84Position {
                latitude_degrees: 0.0,
                longitude_degrees: 0.0,
                ellipsoid_height_m: 0.0,
            })],
            allow_approximation: false,
        },
        artifact: false,
    })
    .unwrap();
    for pointer in ["", "/convert", "/convert/target", "/convert/points/0"] {
        rejects_extra::<veoveo_frames_mcp::contract::BatchTransformRequest>(batch.clone(), pointer);
    }
    let time = json!({"expression":{"format":"rfc3339", "value":"2026-10-04T00:00:00Z"}});
    for pointer in ["", "/expression"] {
        rejects_extra::<veoveo_time_mcp::contract::ResolveTimeRequest>(time.clone(), pointer);
    }
    use veoveo_map_mcp::contract::{GeodesicInverseRequest, Wgs84Position as MapWgs84Position};
    rejects_extra::<GeodesicInverseRequest>(
        serde_json::to_value(GeodesicInverseRequest {
            start: MapWgs84Position::new(0.0, 0.0, None).unwrap(),
            end: MapWgs84Position::new(1.0, 1.0, None).unwrap(),
        })
        .unwrap(),
        "",
    );
    rejects_extra::<veoveo_view_mcp::contract::CloseViewRequest>(
        json!({"view_id":"view-a", "expected_revision":1}),
        "",
    );
    rejects_extra::<veoveo_uav_sim_mcp::contract::VehicleRequest>(
        json!({"session_id":"session-a", "vehicle_id":"vehicle-a"}),
        "",
    );
}

#[test]
fn object_union_unit_and_data_variants_reject_unknown_tags_and_fields() {
    use veoveo_map_mcp::contract::{FeatureExportFormat, FeatureGeometry, TravelTimeModel};
    for value in [
        json!({"format":"geo_json_seq"}),
        json!({"format":"geo_parquet"}),
        json!({"format":"geo_package", "table":"features"}),
    ] {
        rejects_extra::<FeatureExportFormat>(value, "");
    }
    rejects_extra::<TravelTimeModel>(json!({"kind":"static"}), "");
    for value in [
        json!({"type":"Point", "coordinates":[0.0,0.0]}),
        json!({"type":"MultiPoint", "coordinates":[[0.0,0.0]]}),
        json!({"type":"LineString", "coordinates":[[0.0,0.0],[1.0,1.0]]}),
        json!({"type":"MultiLineString", "coordinates":[[[0.0,0.0],[1.0,1.0]]]}),
        json!({"type":"Polygon", "coordinates":[[[0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,0.0]]]}),
        json!({"type":"MultiPolygon", "coordinates":[[[[0.0,0.0],[1.0,0.0],[1.0,1.0],[0.0,0.0]]]]}),
    ] {
        rejects_extra::<FeatureGeometry>(value, "");
    }
    rejects_tag::<FeatureExportFormat>(json!({"format":"unknown"}));
    rejects_tag::<TravelTimeModel>(json!({"kind":"unknown"}));
    rejects_tag::<FeatureGeometry>(json!({"type":"Unknown", "coordinates":[]}));
}

fn rejects_tag<T: JsonSchema + DeserializeOwned>(value: Value) {
    let validator = jsonschema::validator_for(&schema::<T>()).unwrap();
    assert!(
        !validator.is_valid(&value),
        "unknown tag accepted by schema: {value}"
    );
    assert!(
        serde_json::from_value::<T>(value).is_err(),
        "unknown tag accepted by decoder"
    );
}

#[test]
fn adjacent_tagged_mobility_reuses_the_owner_fixture() {
    use veoveo_map_mcp::contract::MobilityProfile;
    let profile: Value = serde_json::from_str(include_str!(
        "../../../../servers/map-mcp/tests/fixtures/mobility.json"
    ))
    .unwrap();
    for pointer in ["", "/profile", "/profile/metadata", "/profile/planning"] {
        rejects_extra::<MobilityProfile>(profile.clone(), pointer);
    }
    let mut unknown = profile;
    unknown["family"] = json!("unknown");
    rejects_tag::<MobilityProfile>(unknown);
}
