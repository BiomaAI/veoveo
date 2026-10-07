use super::*;
use chrono::{DateTime, Utc};
use serde_json::json;

pub(crate) fn now() -> DateTime<Utc> {
    "2026-09-28T10:00:00Z".parse().unwrap()
}

pub(crate) fn authority() -> SceneCompositionAuthority {
    serde_json::from_value(json!({
        "principalId": "operator",
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
        "schemaVersion": SCENE_COMPOSITION_SCHEMA_VERSION, "baseLayer":"base", "styleId":"default:1",
        "governedInputs": [{"inputId":"geometry", "resourceUri":uri, "digestSha256":Sha256Digest::from_bytes(b"source"), "license":"Test", "attribution":"Fixture"}],
        "overlays": [{"overlayId":"marker", "governedInputIds":["geometry"], "geometry":{
            "kind":"inline", "geometry":{"kind":"marker", "position":{"kind":"wgs84", "position":{
                "latitudeDegrees":13.723645678901234, "longitudeDegrees":-89.212_312_312_312_3, "ellipsoidalHeightMeters":100.12345678912345
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

/// Replacement, mixed and conflicting retired keys at each actual owned producer field.
pub(crate) fn retired_field_cases(value: &serde_json::Value) -> Vec<(String, serde_json::Value)> {
    fn fields(value: &serde_json::Value, path: &str, out: &mut Vec<(String, String, String)>) {
        match value {
            serde_json::Value::Object(object) => {
                for (key, child) in object {
                    if key.chars().any(char::is_uppercase) {
                        let old = key.chars().fold(String::new(), |mut old, c| {
                            if c.is_uppercase() {
                                old.push('_');
                                old.extend(c.to_lowercase());
                            } else {
                                old.push(c);
                            }
                            old
                        });
                        out.push((path.into(), key.clone(), old));
                    }
                    fields(child, &format!("{path}/{key}"), out);
                }
            }
            serde_json::Value::Array(array) => {
                for (index, child) in array.iter().enumerate() {
                    fields(child, &format!("{path}/{index}"), out);
                }
            }
            _ => {}
        }
    }
    let mut all = Vec::new();
    fields(value, "", &mut all);
    let mut cases = Vec::new();
    for (path, current, retired) in all {
        for mode in ["replacement", "mixed", "conflicting"] {
            let mut invalid = value.clone();
            let object = invalid.pointer_mut(&path).unwrap().as_object_mut().unwrap();
            let field = object[&current].clone();
            if mode == "replacement" {
                object.remove(&current);
            }
            object.insert(
                retired.clone(),
                if mode == "conflicting" {
                    json!("retired conflict")
                } else {
                    field
                },
            );
            cases.push((format!("{path}/{retired} {mode}"), invalid));
        }
    }
    cases
}
