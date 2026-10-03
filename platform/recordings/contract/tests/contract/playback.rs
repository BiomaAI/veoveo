use serde_json::{Value, json};
use veoveo_recording_contract::{
    PLAYBACK_MANIFEST_SCHEMA, PlaybackArchiveUri, PlaybackManifest, PlaybackManifestBuilder,
    PlaybackManifestSchema, RecordingDatasetId, RecordingId, RecordingReadGrantId,
    RecordingRedapOrigin, RecordingState,
};

fn manifest() -> Value {
    let dataset = RecordingDatasetId::new();
    let recording = RecordingId::new();
    let origin = RecordingRedapOrigin::from_http("https://archive.example").unwrap();
    json!({
        "schema": PLAYBACK_MANIFEST_SCHEMA, "dataset_id": dataset,
        "recording_segment_id": recording, "application_id": "producer",
        "recording_key": "capture", "state": "sealed",
        "started_at": "2026-09-29T00:00:00Z", "ended_at": "2026-09-29T00:01:00Z",
        "catalog_revision": "r1",
        "access": { "grant_id": RecordingReadGrantId::new(), "redap_token": "opaque-token",
            "expires_at": "2026-09-29T00:05:00Z" },
        "archive": { "uri": PlaybackArchiveUri::new(&origin, dataset, recording), "dataset_id": dataset,
            "recording_segment_id": recording, "catalog_revision": "r1", "rrd_version": "0.38.1",
            "optimization_profile": "object-store", "byte_len": 128, "layer_count": 1 },
        "live": null,
        "blueprint": { "blueprint_id": "producer-blueprint", "revision": 1,
            "sha256": "a".repeat(64), "byte_len": 64, "map_provider": "none" }
    })
}

#[test]
fn checked_playback_construction_and_json_share_the_wire_model() {
    let wire = manifest();
    let builder: PlaybackManifestBuilder = serde_json::from_value(wire.clone()).unwrap();
    let admitted = builder.build().unwrap();
    assert_eq!(admitted.schema, PlaybackManifestSchema::V10);
    assert_eq!(admitted.state, RecordingState::Sealed);
    assert_eq!(
        admitted.blueprint.as_ref().unwrap().sha256.hex(),
        "a".repeat(64)
    );
    assert_eq!(serde_json::to_value(&admitted).unwrap(), wire);
    let decoded: PlaybackManifest = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), wire);
    let schema = serde_json::to_value(schemars::schema_for!(PlaybackManifest)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["$defs"]["RecordingState"]["enum"],
        json!([
            "live",
            "ready",
            "sealing",
            "sealed",
            "interrupted",
            "failed"
        ])
    );
}

#[test]
fn manifest_admission_rejects_wrong_parents_shapes_and_values() {
    let wire = manifest();
    let origin = RecordingRedapOrigin::from_http("https://archive.example").unwrap();
    let dataset: RecordingDatasetId = serde_json::from_value(wire["dataset_id"].clone()).unwrap();
    let recording: RecordingId =
        serde_json::from_value(wire["recording_segment_id"].clone()).unwrap();
    for (pointer, invalid) in [
        ("/schema", json!("veoveo.ai/recording-playback/v9")),
        ("/state", json!("recording")),
        ("/application_id", json!(" ")),
        ("/recording_key", json!("bad\nkey")),
        ("/catalog_revision", json!("")),
        ("/access/redap_token", json!("")),
        ("/access/expires_at", json!("not-a-date")),
        ("/started_at", json!("not-a-date")),
        ("/ended_at", json!("2026-09-28T23:00:00Z")),
        ("/archive/dataset_id", json!(RecordingDatasetId::new())),
        ("/archive/recording_segment_id", json!(RecordingId::new())),
        ("/archive/catalog_revision", json!("different")),
        ("/archive/uri", json!("")),
        (
            "/archive/uri",
            json!(PlaybackArchiveUri::new(
                &origin,
                RecordingDatasetId::new(),
                recording
            )),
        ),
        (
            "/archive/uri",
            json!(PlaybackArchiveUri::new(
                &origin,
                dataset,
                RecordingId::new()
            )),
        ),
        ("/archive/layer_count", json!(0)),
        ("/archive/byte_len", json!(0)),
        ("/blueprint/blueprint_id", json!("")),
        ("/blueprint/revision", json!(0)),
        ("/blueprint/byte_len", json!(0)),
        ("/blueprint/sha256", json!("a".repeat(63))),
        ("/blueprint/sha256", json!("A".repeat(64))),
        (
            "/blueprint/sha256",
            json!(format!("sha256:{}", "a".repeat(64))),
        ),
    ] {
        let mut invalid_wire = wire.clone();
        *invalid_wire.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            serde_json::from_value::<PlaybackManifest>(invalid_wire.clone()).is_err(),
            "{pointer}"
        );
        // Values that reach a typed Rust builder must still fail relationship admission.
        if let Ok(builder) = serde_json::from_value::<PlaybackManifestBuilder>(invalid_wire) {
            assert!(builder.build().is_err(), "{pointer}");
        }
    }
    for pointer in ["", "/access", "/archive", "/blueprint"] {
        let mut unknown = wire.clone();
        unknown.pointer_mut(pointer).unwrap()["unknown"] = json!(true);
        assert!(serde_json::from_value::<PlaybackManifest>(unknown).is_err());
    }
}

#[test]
fn lifecycle_requires_the_recording_scoped_live_channel() {
    let mut live = manifest();
    live["state"] = json!("live");
    live["ended_at"] = Value::Null;
    live["live"] = json!({
        "history_seconds": 1,
        "video_preroll_seconds": 2, "transport": "rerun_rrd_channel_v2"
    });
    assert!(serde_json::from_value::<PlaybackManifest>(live.clone()).is_err());
    live["archive"] = Value::Null;
    assert!(serde_json::from_value::<PlaybackManifest>(live.clone()).is_ok());
    for (pointer, invalid) in [
        ("/live", Value::Null),
        ("/state", json!("sealed")),
        ("/ended_at", json!("2026-09-29T00:01:00Z")),
        ("/live/history_seconds", json!(0)),
        ("/live/video_preroll_seconds", json!(0)),
    ] {
        let mut invalid_wire = live.clone();
        *invalid_wire.pointer_mut(pointer).unwrap() = invalid;
        assert!(
            serde_json::from_value::<PlaybackManifest>(invalid_wire).is_err(),
            "{pointer}"
        );
    }
    for state in ["ready", "sealing", "sealed", "interrupted", "failed"] {
        let mut terminal = live.clone();
        terminal["state"] = json!(state);
        assert!(serde_json::from_value::<PlaybackManifest>(terminal.clone()).is_err());
        terminal["live"] = Value::Null;
        assert!(serde_json::from_value::<PlaybackManifest>(terminal).is_ok());
    }
    // Capture-layer details belong to the server's current-layer selection, not
    // a recording receiver. Stale versioned payloads must not be accepted.
    for field in ["layer_id", "layer_name", "ordinal", "current_byte_len"] {
        let mut unknown = live.clone();
        unknown["live"][field] = json!(0);
        assert!(serde_json::from_value::<PlaybackManifest>(unknown).is_err());
    }
}
