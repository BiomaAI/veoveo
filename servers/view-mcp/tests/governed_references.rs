use serde_json::json;
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_frames_mcp::contract::{CoordinateOperationId, FrameOperationUri};
use veoveo_map_mcp::contract::*;
use veoveo_recording_contract::{RecordingId, RecordingUri};
use veoveo_view_mcp::contract::{
    CreateSceneCompositionRequest, GovernedResourceUri, SceneCompositionError,
};

#[test]
fn governed_addresses_preserve_the_owner_types_and_string_wire_shape() {
    let artifact = ArtifactId::new();
    for address in [
        GovernedResourceUri::Artifact(artifact.plane_uri()),
        GovernedResourceUri::Artifact(ArtifactUri::presented(
            &veoveo_map_mcp::uris::SCHEME,
            artifact,
        )),
        GovernedResourceUri::SourceFeature(MapSourceFeatureUri::new(
            DatasetReleaseId::new(),
            SourceFeatureId::new(),
        )),
        GovernedResourceUri::Raster(MapRasterUri::new(RasterProductId::new())),
        GovernedResourceUri::RasterDerivation(MapRasterDerivationUri::new(
            RasterDerivationId::new(),
        )),
        GovernedResourceUri::SpatialDerivation(MapSpatialDerivationUri::new(
            SpatialDerivationId::new(),
        )),
        GovernedResourceUri::Route(MapRouteUri::new(RouteId::new())),
        GovernedResourceUri::Recording(RecordingUri::new(RecordingId::new())),
        GovernedResourceUri::FrameOperation(FrameOperationUri::new(
            &CoordinateOperationId::parse("operation-1").unwrap(),
        )),
    ] {
        assert_eq!(
            GovernedResourceUri::parse(address.as_str()).unwrap(),
            address
        );
        let wire = serde_json::to_value(&address).unwrap();
        assert_eq!(wire, address.as_str());
        assert_eq!(
            serde_json::from_value::<GovernedResourceUri>(wire).unwrap(),
            address
        );
        for suffix in ["/extra", "?token=secret", "#secret"] {
            assert!(GovernedResourceUri::parse(format!("{address}{suffix}")).is_err());
        }
    }
    assert_eq!(
        schemars::schema_for!(GovernedResourceUri).as_value()["type"],
        "string"
    );
}

#[test]
fn admission_rejects_unpublished_recording_routes_and_arbitrary_product_paths() {
    let id = RecordingId::new();
    for input in [
        format!("recording://recording/{id}"),
        format!("recording://recordings/{id}/layers"),
        "recording://recordings".into(),
        "recording://recordings/not-an-id".into(),
        "map://route/route-1".into(),
        "map://source-feature/extra/arbitrary/secret".into(),
        "map://raster/arbitrary".into(),
        "map://spatial-derivation/arbitrary".into(),
        "map://raster-derivation/arbitrary".into(),
        "frames://operation/operation-1/secret".into(),
        "frames://world/world/revision/revision".into(),
    ] {
        let error = GovernedResourceUri::parse(input).unwrap_err();
        assert_eq!(error, SceneCompositionError::InvalidGovernedResourceUri);
        assert!(!error.to_string().contains("secret"));
    }
}

fn request(address: GovernedResourceUri) -> CreateSceneCompositionRequest {
    serde_json::from_value(json!({
        "schema_version": 1, "base_layer": "fixture", "style_id": "fixture",
        "governed_inputs": [{"input_id": "source", "resource_uri": address,
            "digest_sha256": "0".repeat(64), "license": "CC0", "attribution": "fixture"}]
    }))
    .unwrap()
}

#[test]
fn source_features_require_their_release_and_presented_map_artifacts_keep_release_admission() {
    let release = DatasetReleaseId::new();
    let mut input = request(GovernedResourceUri::SourceFeature(
        MapSourceFeatureUri::new(release.clone(), SourceFeatureId::new()),
    ));
    assert_eq!(
        input.validate().unwrap_err(),
        SceneCompositionError::MapReleaseRequired
    );
    input.map_releases.insert(MapReleaseUri::new(
        MapDatasetId::new(),
        DatasetReleaseId::new(),
    ));
    assert_eq!(
        input.validate().unwrap_err(),
        SceneCompositionError::MapReleaseMismatch
    );
    input
        .map_releases
        .insert(MapReleaseUri::new(MapDatasetId::new(), release));
    input.validate().unwrap();
    let artifact = GovernedResourceUri::Artifact(ArtifactUri::presented(
        &veoveo_map_mcp::uris::SCHEME,
        ArtifactId::new(),
    ));
    assert_eq!(
        request(artifact).validate().unwrap_err(),
        SceneCompositionError::MapReleaseRequired
    );
    request(GovernedResourceUri::Recording(RecordingUri::new(
        RecordingId::new(),
    )))
    .validate()
    .unwrap();
}
