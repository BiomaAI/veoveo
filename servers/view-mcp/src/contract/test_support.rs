use super::*;
use chrono::{DateTime, Utc};
use serde_json::json;

pub(crate) fn now() -> DateTime<Utc> {
    "2026-09-28T10:00:00Z".parse().unwrap()
}

pub(crate) fn authority() -> SceneCompositionAuthority {
    serde_json::from_value(json!({
        "principal_id": "operator",
        "invocation": {
            "work_context": "operations", "tenant": "tenant", "membership": "custodian", "policy_revision": "r1",
            "output_policy": {"owner":{"kind":"principal","id":"operator"}, "initial_grants":[], "classification":"internal", "data_labels":[]},
            "provenance": {"mode":"delegated", "initiator":"operator", "delegation_id":"capture"}
        }
    })).unwrap()
}

pub(crate) fn request() -> CreateSceneCompositionRequest {
    let uri = veoveo_artifact_contract::ArtifactUri::plane(
        "0197f78e-f2f0-7a6e-8a5d-f41c691e4471".parse().unwrap(),
    );
    serde_json::from_value(json!({
        "schema_version": SCENE_COMPOSITION_SCHEMA_VERSION, "base_layer":"base", "style_id":"default:1",
        "governed_inputs": [{"input_id":"geometry", "resource_uri":uri, "digest_sha256":Sha256Digest::from_bytes(b"source"), "license":"Test", "attribution":"Fixture"}],
        "overlays": [{"overlay_id":"marker", "governed_input_ids":["geometry"], "geometry":{
            "kind":"inline", "geometry":{"kind":"marker", "position":{"kind":"wgs84", "position":{
                "latitude_degrees":13.723645678901234, "longitude_degrees":-89.212_312_312_312_3, "ellipsoidal_height_meters":100.12345678912345
            }}}
        }}]
    })).unwrap()
}

pub(crate) fn composition() -> SceneComposition {
    SceneComposition::new(request(), authority(), now()).unwrap()
}

pub(crate) fn camera() -> CameraDefinition {
    CameraDefinition::Pose(GeodeticCameraPose {
        position: Wgs84Position3d {
            latitude_degrees: 13.723456789012345,
            longitude_degrees: -89.21234567891234,
            ellipsoidal_height_meters: 1000.1234567891234,
        },
        orientation: HeadingPitchRoll {
            heading_degrees: 180.0,
            pitch_degrees: -45.0,
            roll_degrees: 0.0,
        },
        vertical_fov_degrees: 45.0,
    })
}

pub(crate) fn view() -> ViewRecord {
    ViewRecord::new(
        ViewId::parse("view-1").unwrap(),
        &composition(),
        camera(),
        now(),
    )
    .unwrap()
}
