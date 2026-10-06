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
        snapshot.digest_sha256().unwrap().hex(),
        "b39375df89fabab90acb1982bea5a06b8c42758f6c02e9c2d0b952701afe95c1"
    );
    let mut reordered = value.clone();
    reordered["sources"].as_array_mut().unwrap().reverse();
    let reordered: RecordingSourceSnapshot = serde_json::from_value(reordered).unwrap();
    assert_ne!(
        snapshot.digest_sha256().unwrap(),
        reordered.digest_sha256().unwrap()
    );
    for (pointer, replacement) in [
        ("/sources/1/part_sequence", serde_json::json!(8)),
        ("/sources/0/byte_len", serde_json::json!(129)),
    ] {
        let mut changed = value.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        let changed: RecordingSourceSnapshot = serde_json::from_value(changed).unwrap();
        assert_ne!(
            snapshot.digest_sha256().unwrap(),
            changed.digest_sha256().unwrap()
        );
    }
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
    let mut wire = serde_json::json!({
        "recording_uri": "recording://recordings/01983da0-0000-7000-8000-000000000000",
        "entity_path": "/camera/front", "timeline": "sensor_time", "range": {"start": 10, "end": 20}
    });
    let selection: RecordingVideoSelection = serde_json::from_value(wire.clone()).unwrap();
    validate_video_selection(&selection).unwrap();
    wire["range"]["end"] = 9.into();
    assert!(serde_json::from_value::<RecordingVideoSelection>(wire.clone()).is_err());
    wire["range"]["end"] = 20.into();
    wire["timeline"] = "sensor\n_time".into();
    assert!(serde_json::from_value::<RecordingVideoSelection>(wire).is_err());
}

#[test]
fn recording_addresses_are_admitted_before_source_materialization() {
    let mut wire = serde_json::json!({
        "recording_uri": "recording://recordings/01983da0-0000-7000-8000-000000000000",
        "entity_path": "/camera/front", "timeline": "sensor_time",
        "range": {"start": 10, "end": 20}
    });
    let selected: RecordingVideoSelection = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(selected).unwrap(), wire);
    for address in [
        "recording://recording/01983da0-0000-7000-8000-000000000000",
        "recording://recordings/01983da0-0000-4000-8000-000000000000",
        "recording://recordings/01983da0-0000-7000-8000-000000000000/layers",
        "recording://recordings/01983da0-0000-7000-8000-000000000000?private=token",
    ] {
        wire["recording_uri"] = address.into();
        let error = serde_json::from_value::<RecordingVideoSelection>(wire.clone()).unwrap_err();
        assert!(!error.to_string().contains(address));
    }
}

#[test]
fn source_admission_checks_ids_digests_parts_and_layer_relationships() {
    use serde_json::json;
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/source-snapshot.json")).unwrap();
    for (pointer, replacement) in [
        (
            "/recording_id",
            json!("01983da0-0000-4000-8000-000000000000"),
        ),
        ("/dataset_id", json!("invalid")),
        ("/sources/0/layer_id", json!("invalid")),
        ("/sources/0/byte_len", json!(0)),
        ("/sources/0/layer_ordinal", json!(-1)),
        ("/sources/0/sha256", json!("A".repeat(64))),
        (
            "/sources/0/sha256",
            json!(format!("sha256:{}", "a".repeat(64))),
        ),
        ("/sources/0/part_sequence", json!(1)),
        ("/sources/1/part_sequence", json!(null)),
        ("/sources/1/layer_name", json!("")),
        ("/sources", json!([])),
    ] {
        let mut invalid = value.clone();
        if pointer == "/sources/0/part_sequence" {
            invalid["sources"][0]["part_sequence"] = replacement;
        } else {
            *invalid.pointer_mut(pointer).unwrap() = replacement;
        }
        assert!(
            serde_json::from_value::<RecordingSourceSnapshot>(invalid).is_err(),
            "{pointer}"
        );
    }
    let mut duplicated = value.clone();
    duplicated["sources"]
        .as_array_mut()
        .unwrap()
        .push(value["sources"][0].clone());
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(duplicated).is_err());
    let mut mixed = value.clone();
    mixed["sources"][1]["layer_id"] = mixed["sources"][0]["layer_id"].clone();
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(mixed).is_err());
    let mut same_name = value.clone();
    same_name["sources"][1]["layer_name"] = same_name["sources"][0]["layer_name"].clone();
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(same_name).is_err());
    let mut overflow = value.clone();
    overflow["sources"][0]["byte_len"] = json!(u64::MAX);
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(overflow).is_err());
    let mut parts = value.clone();
    let mut part = value["sources"][1].clone();
    part["part_sequence"] = json!(8);
    parts["sources"].as_array_mut().unwrap().push(part);
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(parts.clone()).is_ok());
    parts["sources"][2]["layer_name"] = json!("changed-layer");
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(parts).is_err());
}

#[test]
fn admitted_source_models_delegate_builder_schema_identity() {
    use schemars::JsonSchema;
    use veoveo_recording_video::contract::{
        IndexRange, IndexRangeBuilder, RecordingSourceIdentity, RecordingSourceIdentityBuilder,
        RecordingSourceSnapshotBuilder, RecordingVideoSelectionBuilder,
    };
    fn same<Model: JsonSchema, Builder: JsonSchema>() {
        assert_eq!(Model::schema_name(), Builder::schema_name());
        assert_eq!(Model::schema_id(), Builder::schema_id());
        assert_eq!(Model::inline_schema(), Builder::inline_schema());
        assert_eq!(schemars::schema_for!(Model), schemars::schema_for!(Builder));
    }
    same::<RecordingSourceSnapshot, RecordingSourceSnapshotBuilder>();
    same::<RecordingSourceIdentity, RecordingSourceIdentityBuilder>();
    same::<RecordingVideoSelection, RecordingVideoSelectionBuilder>();
    same::<IndexRange, IndexRangeBuilder>();
}
