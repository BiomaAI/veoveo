use veoveo_recording_video::contract::{
    RecordingSourceSnapshot, RecordingVideoSelection, validate_video_selection,
};

fn source_snapshot() -> RecordingSourceSnapshot {
    use veoveo_recording_video::contract::{
        RecordingSourceIdentityBuilder, RecordingSourceIdentityKind, RecordingSourceSnapshotBuilder,
    };
    let source = |id: &str, name: &str, ordinal, kind, part, bytes, digest| {
        RecordingSourceIdentityBuilder {
            layer_id: id.parse().unwrap(),
            layer_name: name.into(),
            layer_ordinal: ordinal,
            kind,
            part_sequence: part,
            byte_len: std::num::NonZeroU64::new(bytes).unwrap(),
            sha256: veoveo_types::Sha256Digest::from_bytes([digest; 32]),
        }
        .build()
        .unwrap()
    };
    RecordingSourceSnapshotBuilder {
        recording_id: "01983da0-0000-7000-8000-000000000000".parse().unwrap(),
        dataset_id: "01983da0-0000-7000-8000-000000000001".parse().unwrap(),
        captured_at: "2026-09-28T00:00:00Z".parse().unwrap(),
        sources: vec![
            source(
                "01983da0-0000-7000-8000-000000000002",
                "archive",
                Some(1),
                RecordingSourceIdentityKind::CommittedLayer,
                None,
                128,
                0xaa,
            ),
            source(
                "01983da0-0000-7000-8000-000000000003",
                "camera/front",
                None,
                RecordingSourceIdentityKind::LiveIngestPart,
                Some(7),
                64,
                0xbb,
            ),
        ],
    }
    .build()
    .unwrap()
}

#[test]
fn captured_source_identity_preserves_wire_bytes_and_digest() {
    let value = serde_json::to_value(source_snapshot()).unwrap();
    if std::env::var_os("UPDATE_RECORDED_VIDEO_FIXTURES").is_some() {
        std::fs::write(
            concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/source-snapshot.json"),
            serde_json::to_string_pretty(&source_snapshot()).unwrap() + "\n",
        )
        .unwrap();
    }
    let captured: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/source-snapshot.json"
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(value, captured);
    let snapshot: RecordingSourceSnapshot = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&snapshot).unwrap(), value);
    assert_eq!(
        serde_json::to_string(&snapshot).unwrap(),
        r#"{"recordingId":"01983da0-0000-7000-8000-000000000000","datasetId":"01983da0-0000-7000-8000-000000000001","capturedAt":"2026-09-28T00:00:00Z","sources":[{"layerId":"01983da0-0000-7000-8000-000000000002","layerName":"archive","layerOrdinal":1,"kind":"committed_layer","byteLen":128,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"layerId":"01983da0-0000-7000-8000-000000000003","layerName":"camera/front","kind":"live_ingest_part","partSequence":7,"byteLen":64,"sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]}"#
    );
    assert_eq!(
        snapshot.digest_sha256().unwrap().hex(),
        "7914587a2144be89bb0a989a3102fdb893520ca91687e9a9a4d14f5bda30e3be"
    );
    let mut reordered = value.clone();
    reordered["sources"].as_array_mut().unwrap().reverse();
    let reordered: RecordingSourceSnapshot = serde_json::from_value(reordered).unwrap();
    assert_ne!(
        snapshot.digest_sha256().unwrap(),
        reordered.digest_sha256().unwrap()
    );
    for (pointer, replacement) in [
        ("/sources/1/partSequence", serde_json::json!(8)),
        ("/sources/0/byteLen", serde_json::json!(129)),
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
    let value = serde_json::to_value(source_snapshot()).unwrap();
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
        "recordingUri": "recording://recordings/01983da0-0000-7000-8000-000000000000",
        "entityPath": "/camera/front", "timeline": "sensor_time", "range": {"start": 10, "end": 20}
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
        "recordingUri": "recording://recordings/01983da0-0000-7000-8000-000000000000",
        "entityPath": "/camera/front", "timeline": "sensor_time",
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
        wire["recordingUri"] = address.into();
        let error = serde_json::from_value::<RecordingVideoSelection>(wire.clone()).unwrap_err();
        assert!(!error.to_string().contains(address));
    }
}

#[test]
fn source_admission_checks_ids_digests_parts_and_layer_relationships() {
    use serde_json::json;
    let value = serde_json::to_value(source_snapshot()).unwrap();
    for (pointer, replacement) in [
        (
            "/recordingId",
            json!("01983da0-0000-4000-8000-000000000000"),
        ),
        ("/datasetId", json!("invalid")),
        ("/sources/0/layerId", json!("invalid")),
        ("/sources/0/byteLen", json!(0)),
        ("/sources/0/layerOrdinal", json!(-1)),
        ("/sources/0/sha256", json!("A".repeat(64))),
        (
            "/sources/0/sha256",
            json!(format!("sha256:{}", "a".repeat(64))),
        ),
        ("/sources/0/partSequence", json!(1)),
        ("/sources/1/partSequence", json!(null)),
        ("/sources/1/layerName", json!("")),
        ("/sources", json!([])),
    ] {
        let mut invalid = value.clone();
        if pointer == "/sources/0/partSequence" {
            invalid["sources"][0]["partSequence"] = replacement;
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
    mixed["sources"][1]["layerId"] = mixed["sources"][0]["layerId"].clone();
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(mixed).is_err());
    let mut same_name = value.clone();
    same_name["sources"][1]["layerName"] = same_name["sources"][0]["layerName"].clone();
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(same_name).is_err());
    let mut overflow = value.clone();
    overflow["sources"][0]["byteLen"] = json!(u64::MAX);
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(overflow).is_err());
    let mut parts = value.clone();
    let mut part = value["sources"][1].clone();
    part["partSequence"] = json!(8);
    parts["sources"].as_array_mut().unwrap().push(part);
    assert!(serde_json::from_value::<RecordingSourceSnapshot>(parts.clone()).is_ok());
    parts["sources"][2]["layerName"] = json!("changed-layer");
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

#[test]
fn source_and_selection_refuse_retired_and_mixed_spelling() {
    let source = serde_json::to_value(source_snapshot()).unwrap();
    for (path, current, retired) in [
        ("", "recordingId", "recording_id"),
        ("", "datasetId", "dataset_id"),
        ("", "capturedAt", "captured_at"),
        ("/sources/0", "layerId", "layer_id"),
        ("/sources/0", "layerName", "layer_name"),
        ("/sources/0", "layerOrdinal", "layer_ordinal"),
        ("/sources/0", "byteLen", "byte_len"),
        ("/sources/1", "partSequence", "part_sequence"),
    ] {
        for mode in 0..3 {
            let mut wire = source.clone();
            let object = wire.pointer_mut(path).unwrap().as_object_mut().unwrap();
            let value = if mode == 2 {
                serde_json::json!("retired-conflict")
            } else {
                object[current].clone()
            };
            if mode == 0 {
                object.remove(current);
            }
            object.insert(retired.into(), value);
            assert!(
                serde_json::from_value::<RecordingSourceSnapshot>(wire).is_err(),
                "{path}/{retired}"
            );
        }
    }
}
