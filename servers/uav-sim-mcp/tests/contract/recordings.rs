fn check_recordings<T>(wire: serde_json::Value, recordings: impl Fn(&T) -> &[RecordingUri])
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let result: T = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(recordings(&result), [uri()]);
    assert_eq!(serde_json::to_value(result).unwrap(), wire);
    for invalid in [
        "recording://recordings/producer-stream",
        "recording://catalog",
        "artifact://unexpected",
    ] {
        let mut wire = wire.clone();
        wire["recording_uris"] = serde_json::json!([invalid]);
        assert!(serde_json::from_value::<T>(wire).is_err());
    }
}
use serde_json::{Value, json};
use veoveo_recording_contract::{RecordingId, RecordingUri};
use veoveo_uav_sim_mcp::contract::{
    CaptureDatasetResult, MissionResult, RecordingCatalog, RecordingCatalogLifecycle, RecordingKey,
    RecordingPublisherLifecycle, RecordingState, ScenarioResult,
};

const ID: &str = "019f7122-3d89-7d21-8312-8940d1e0f510";
const OTHER_ID: &str = "019f7122-3d89-7d21-8312-8940d1e0f511";
fn uri() -> RecordingUri {
    RecordingUri::new(RecordingId::parse(ID).unwrap())
}
fn state(catalog: RecordingCatalog) -> RecordingState {
    RecordingState {
        recording_key: RecordingKey::parse("producer-stream").unwrap(),
        catalog,
        active: true,
        publisher_lifecycle: RecordingPublisherLifecycle::Ready,
        queue_capacity: 100,
        queued_events: 4,
        dropped_events: 0,
        publisher_diagnostic: None,
        camera_streams: vec!["/camera".into()],
        started_at: "2026-09-28T12:00:00Z".parse().unwrap(),
    }
}

#[test]
fn catalog_construction_derives_identity_and_preserves_wire_fields() {
    let ready = state(RecordingCatalog::Ready(uri()));
    assert_eq!(ready.catalog.recording_id(), Some(uri().id()));
    assert_eq!(ready.catalog.lifecycle(), RecordingCatalogLifecycle::Ready);
    let wire = serde_json::to_value(&ready).unwrap();
    assert_eq!(
        wire,
        json!({
            "recording_key": "producer-stream", "catalog_lifecycle": "ready",
            "recording_id": ID, "recording_uri": uri(), "active": true,
            "publisher_lifecycle": "ready", "queue_capacity": 100, "queued_events": 4,
            "dropped_events": 0, "camera_streams": ["/camera"], "started_at": "2026-09-28T12:00:00Z"
        })
    );
    assert_eq!(
        serde_json::from_value::<RecordingState>(wire).unwrap(),
        ready
    );
    for catalog in [
        RecordingCatalog::Pending {
            diagnostic: Some("pending".into()),
        },
        RecordingCatalog::Unavailable { diagnostic: None },
        RecordingCatalog::Invalid {
            diagnostic: Some("invalid".into()),
        },
    ] {
        let value = state(catalog);
        let wire = serde_json::to_value(&value).unwrap();
        assert!(wire.get("recording_id").is_none());
        assert!(wire.get("recording_uri").is_none());
        assert_eq!(
            serde_json::from_value::<RecordingState>(wire).unwrap(),
            value
        );
    }
}

#[test]
fn catalog_admission_rejects_conflicting_partial_and_malformed_identities() {
    let ready = serde_json::to_value(state(RecordingCatalog::Ready(uri()))).unwrap();
    let mut invalid = Vec::new();
    for (field, value) in [
        ("recording_id", json!(OTHER_ID)),
        ("recording_id", Value::Null),
        ("recording_id", json!("producer-stream")),
        ("recording_uri", Value::Null),
        ("recording_uri", json!(format!("{}/layers", uri()))),
        ("recording_uri", json!(format!("{}?private=value", uri()))),
        ("catalog_diagnostic", json!("private-diagnostic")),
        ("catalog_lifecycle", json!("pending")),
        ("catalog_lifecycle", json!("unavailable")),
        ("catalog_lifecycle", json!("invalid")),
    ] {
        let mut wire = ready.clone();
        wire[field] = value;
        invalid.push(wire);
    }
    for field in ["recording_id", "recording_uri"] {
        let mut wire = ready.clone();
        wire.as_object_mut().unwrap().remove(field);
        invalid.push(wire);
    }
    for wire in invalid {
        let error = serde_json::from_value::<RecordingState>(wire).unwrap_err();
        let message = error.to_string();
        assert!(!message.contains(ID));
        assert!(!message.contains("private"));
    }
}

#[test]
fn durable_results_admit_only_recording_owner_addresses() {
    let scenario = json!({"session_id":"s", "elapsed_seconds":1.0,
        "final_simulation_time_s":2.0, "collision_count":0, "recording_uris":[uri()]});
    let mission = json!({"mission_id":"m", "lifecycle":"completed",
        "started_at":"2026-09-28T12:00:00Z", "finished_at":"2026-09-28T12:01:00Z",
        "completed_waypoints":2, "recording_uris":[uri()]});
    let capture = json!({"session_id":"s", "elapsed_seconds":1.0, "recording_uris":[uri()]});

    check_recordings::<ScenarioResult>(scenario, |result| &result.recording_uris);
    check_recordings::<MissionResult>(mission, |result| &result.recording_uris);
    check_recordings::<CaptureDatasetResult>(capture, |result| &result.recording_uris);
}
