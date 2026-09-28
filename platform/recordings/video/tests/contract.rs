use veoveo_recording_video::contract::{
    RecordingSourceSnapshot, RecordingVideoSelection, validate_video_selection,
};

#[test]
fn captured_source_identity_preserves_wire_bytes_and_digest() {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/source-snapshot.json")).unwrap();
    let snapshot: RecordingSourceSnapshot = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&snapshot).unwrap(), value);
    assert_eq!(
        snapshot.digest_sha256().unwrap(),
        "b39375df89fabab90acb1982bea5a06b8c42758f6c02e9c2d0b952701afe95c1"
    );
    let mut reordered = snapshot.clone();
    reordered.sources.reverse();
    assert_ne!(
        snapshot.digest_sha256().unwrap(),
        reordered.digest_sha256().unwrap()
    );
    let mut changed = snapshot.clone();
    changed.sources[1].part_sequence = Some(8);
    assert_ne!(
        snapshot.digest_sha256().unwrap(),
        changed.digest_sha256().unwrap()
    );
    let mut changed = snapshot.clone();
    changed.sources[0].byte_len += 1;
    assert_ne!(
        snapshot.digest_sha256().unwrap(),
        changed.digest_sha256().unwrap()
    );
}

#[test]
fn source_identity_rejects_unknown_fields_and_kinds() {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/source-snapshot.json")).unwrap();
    let mut unknown_field = value.clone();
    unknown_field["sources"][0]["path"] = "/local/private/source.rrd".into();
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(unknown_field).is_err());
    let mut unknown_kind = value;
    unknown_kind["sources"][0]["kind"] = "unverified_file".into();
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(unknown_kind).is_err());
}

#[test]
fn selection_validation_is_available_without_materialization() {
    let mut selection: RecordingVideoSelection = serde_json::from_value(serde_json::json!({
        "recording_uri": "recording://recordings/01983da0-0000-7000-8000-000000000000",
        "entity_path": "/camera/front",
        "timeline": "sensor_time",
        "range": {"start": 10, "end": 20}
    }))
    .unwrap();
    validate_video_selection(&selection).unwrap();
    selection.range.end = 9;
    assert!(validate_video_selection(&selection).is_err());
    selection.range.end = 20;
    selection.timeline = "sensor\n_time".into();
    assert!(validate_video_selection(&selection).is_err());
}
