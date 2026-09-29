use super::*;
use crate::contract::{LookAtCamera, OrbitTargetCamera, test_support as fixture};
use serde_json::json;

#[test]
fn view_decoder_rejects_disagreeing_addresses_revisions_timestamps_and_pose() {
    let value = serde_json::to_value(fixture::view()).unwrap();
    for (path, replacement) in [
        (
            "/view_uri",
            json!(ViewUri::new(ViewId::new("other").unwrap())),
        ),
        (
            "/composition_uri",
            json!(CompositionUri::new(SceneCompositionId::from_stable_key(
                b"other"
            ))),
        ),
        ("/revision", json!(0)),
        ("/updated_at", json!("2026-09-27T10:00:00Z")),
        ("/resolved_camera/position/latitude_degrees", json!(80.0)),
        ("/resolved_camera/vertical_fov_degrees", json!(50.0)),
        ("/camera/position/latitude_degrees", json!(91.0)),
    ] {
        let mut invalid = value.clone();
        *invalid.pointer_mut(path).unwrap() = replacement;
        assert!(
            serde_json::from_value::<ViewRecord>(invalid).is_err(),
            "accepted {path}"
        );
    }
}

#[test]
fn every_camera_rig_round_trips_and_cannot_substitute_a_saved_pose() {
    let target = fixture::view().resolved_camera().position;
    let mut eye = target;
    eye.ellipsoidal_height_meters += 1000.0;
    for camera in [
        fixture::camera(),
        CameraDefinition::LookAt(LookAtCamera {
            eye,
            target,
            vertical_fov_degrees: 45.0,
        }),
        CameraDefinition::OrbitTarget(OrbitTargetCamera {
            target,
            distance_meters: 1000.0,
            azimuth_degrees: 180.0,
            elevation_degrees: 30.0,
            vertical_fov_degrees: 45.0,
        }),
    ] {
        let view = ViewRecord::new(
            ViewId::new("view-1").unwrap(),
            &fixture::composition(),
            camera,
            fixture::now(),
        )
        .unwrap();
        let encoded = serde_json::to_vec(&view).unwrap();
        let decoded: ViewRecord = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), encoded);
        let mut invalid = serde_json::to_value(view).unwrap();
        invalid["resolved_camera"]["orientation"]["pitch_degrees"] = json!(20.0);
        assert!(serde_json::from_value::<ViewRecord>(invalid).is_err());
    }
}

#[test]
fn camera_replacement_is_atomic_on_invalid_pose_clock_or_revision_overflow() {
    let mut view = fixture::view();
    let before = serde_json::to_value(&view).unwrap();
    let mut bad = view.resolved_camera().clone();
    bad.position.latitude_degrees = f64::NAN;
    assert!(
        view.replace_camera(CameraDefinition::Pose(bad), fixture::now())
            .is_err()
    );
    assert_eq!(serde_json::to_value(&view).unwrap(), before);
    assert!(
        view.replace_camera(
            fixture::camera(),
            fixture::now() - chrono::Duration::seconds(1)
        )
        .is_err()
    );
    assert_eq!(serde_json::to_value(&view).unwrap(), before);
    let mut exhausted = before;
    exhausted["revision"] = json!(u64::MAX);
    let mut view: ViewRecord = serde_json::from_value(exhausted.clone()).unwrap();
    assert!(matches!(
        view.replace_camera(fixture::camera(), fixture::now()),
        Err(ViewRecordError::Revision)
    ));
    assert_eq!(serde_json::to_value(view).unwrap(), exhausted);
}

#[test]
fn replacing_camera_advances_revision_and_preserves_composition() {
    let mut view = fixture::view();
    let id = view.composition_id().clone();
    let updated = fixture::now() + chrono::Duration::seconds(1);
    let mut pose = view.resolved_camera().clone();
    pose.orientation.heading_degrees = 90.0;
    view.replace_camera(CameraDefinition::Pose(pose.clone()), updated)
        .unwrap();
    assert_eq!(view.revision(), 2);
    assert_eq!(view.resolved_camera(), &pose);
    assert_eq!(view.composition_id(), &id);
    assert_eq!(view.updated_at(), updated);
    assert_eq!(view.created_at(), fixture::now());
    let schema = schemars::schema_for!(ViewRecord);
    jsonschema::validator_for(schema.as_value())
        .unwrap()
        .validate(&serde_json::to_value(view).unwrap())
        .unwrap();
}

#[test]
fn view_creation_cannot_precede_its_composition() {
    assert!(
        ViewRecord::new(
            ViewId::new("view-1").unwrap(),
            &fixture::composition(),
            fixture::camera(),
            fixture::now() - chrono::Duration::seconds(1)
        )
        .is_err()
    );
}
