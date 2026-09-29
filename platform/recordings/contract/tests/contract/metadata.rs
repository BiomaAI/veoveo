//! Public catalog and manifest admission, independent of Store and MCP runtimes.
use serde_json::{Value, json};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_recording_contract::{
    LayerView, LayerViewBuilder, ManifestLayer, ManifestLayerBuilder, RecordingManifest,
    RecordingManifestBuilder, RecordingManifestSchema, RecordingView, SealRecordingOutput,
    SealRecordingOutputBuilder,
};

fn artifact() -> ArtifactUri {
    ArtifactUri::plane(ArtifactId::new())
}
fn layer() -> Value {
    json!({
        "layer_id": super::RECORDING, "layer_name": "capture-00000000000000000000",
        "kind": "capture", "ordinal": 0, "state": "committed", "byte_len": 1024,
        "message_count": 3, "sha256": "a".repeat(64), "artifact_uri": artifact(),
        "rrd_version": "0.38.1", "schema_digest": "b".repeat(64),
        "created_at": "2026-09-29T10:00:00Z", "updated_at": "2026-09-29T10:01:00Z"
    })
}
fn manifest_layer() -> Value {
    let mut wire = layer();
    for key in ["state", "message_count", "created_at", "updated_at"] {
        wire.as_object_mut().unwrap().remove(key);
    }
    wire
}
fn catalog() -> Value {
    json!({
        "recording_id": super::RECORDING, "dataset_id": super::DATASET,
        "dataset_key": "recordings", "application_id": "application", "recording_key": "capture",
        "state": "sealed", "classification": "unclassified", "labels": ["operations"],
        "started_at": "2026-09-29T10:00:00Z", "last_data_at": "2026-09-29T10:01:00Z",
        "ended_at": "2026-09-29T10:01:00Z", "sealed_at": "2026-09-29T10:02:00Z",
        "manifest_artifact_uri": artifact(), "layer_count": 1, "committed_layer_count": 1
    })
}

#[test]
fn layer_admission_checks_lifecycle_integrity_and_kind_identity() {
    let wire = layer();
    let admitted: LayerView = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(admitted.sha256.as_ref().unwrap().hex(), "a".repeat(64));
    assert_eq!(serde_json::to_value(&admitted).unwrap(), wire);
    for (field, value) in [
        ("sha256", json!("a".repeat(63))),
        ("sha256", json!("A".repeat(64))),
        ("schema_digest", Value::Null),
        ("rrd_version", Value::Null),
        ("artifact_uri", Value::Null),
        ("artifact_uri", json!("artifact://bad")),
        ("message_count", json!(0)),
        ("byte_len", json!(-1)),
        ("byte_len", json!(0)),
        ("kind", json!("other")),
        ("kind", json!("derived")),
        ("ordinal", json!(1)),
        ("updated_at", json!("2026-09-29T09:00:00Z")),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<LayerView>(invalid).is_err(),
            "{field}"
        );
    }
    let mut staged = wire.clone();
    staged["state"] = json!("staged");
    assert!(serde_json::from_value::<LayerView>(staged.clone()).is_err());
    staged["artifact_uri"] = Value::Null;
    assert!(serde_json::from_value::<LayerView>(staged).is_ok());
    let mut writing = wire;
    writing["state"] = json!("writing");
    for key in ["sha256", "schema_digest", "rrd_version", "artifact_uri"] {
        writing[key] = Value::Null;
    }
    writing["byte_len"] = json!(0);
    writing["message_count"] = json!(0);
    assert!(serde_json::from_value::<LayerView>(writing.clone()).is_ok());
    writing["state"] = json!("failed");
    assert!(serde_json::from_value::<LayerView>(writing.clone()).is_ok());
    writing["kind"] = json!("properties");
    writing["layer_name"] = json!("properties");
    writing["ordinal"] = Value::Null;
    assert!(serde_json::from_value::<LayerView>(writing.clone()).is_ok());
    writing["kind"] = json!("derived");
    writing["layer_name"] = json!("derived-sensors");
    assert!(serde_json::from_value::<LayerView>(writing).is_ok());
    let mut builder: LayerViewBuilder = (*admitted).clone();
    builder.sha256 = None;
    assert!(builder.build().is_err());
}

#[test]
fn catalog_admission_checks_publication_counts_and_timestamps() {
    let wire = catalog();
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<RecordingView>(wire.clone()).unwrap())
            .unwrap(),
        wire
    );
    for (field, value) in [
        ("manifest_artifact_uri", json!("artifact://invalid")),
        ("manifest_artifact_uri", Value::Null),
        ("sealed_at", Value::Null),
        ("ended_at", Value::Null),
        ("layer_count", json!(0)),
        ("committed_layer_count", json!(0)),
        ("committed_layer_count", json!(2)),
        ("labels", json!(["operations", "operations"])),
        ("labels", json!(["private", "operations"])),
        ("labels", json!(["x".repeat(257)])),
        ("started_at", json!("2026-09-29T11:00:00Z")),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<RecordingView>(invalid).is_err(),
            "{field}"
        );
    }
    let mut live = wire;
    live["state"] = json!("live");
    assert!(serde_json::from_value::<RecordingView>(live.clone()).is_err());
    for key in ["manifest_artifact_uri", "ended_at", "sealed_at"] {
        live[key] = Value::Null;
    }
    live["layer_count"] = json!(0);
    live["committed_layer_count"] = json!(0);
    assert!(serde_json::from_value::<RecordingView>(live).is_ok());
}

#[test]
fn seal_output_rejects_duplicate_occurrences_regardless_of_uri_spelling() {
    let layer = artifact();
    let mut builder = SealRecordingOutputBuilder {
        recording_id: super::RECORDING.parse().unwrap(),
        manifest_artifact_uri: artifact(),
        layer_artifact_uris: vec![layer.clone()],
        blueprint_artifact_uri: Some(artifact()),
    };
    let valid = builder.clone().build().unwrap();
    let wire = serde_json::to_value(&valid).unwrap();
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<SealRecordingOutput>(wire).unwrap()).unwrap(),
        serde_json::to_value(valid).unwrap()
    );
    builder.blueprint_artifact_uri = Some(
        ArtifactUri::parse(
            &layer
                .to_string()
                .to_uppercase()
                .replace("ARTIFACT:", "artifact:"),
        )
        .unwrap(),
    );
    assert!(builder.clone().build().is_err());
    assert!(
        serde_json::from_value::<SealRecordingOutput>(serde_json::to_value(&builder).unwrap())
            .is_err()
    );
    builder.blueprint_artifact_uri = None;
    builder.layer_artifact_uris.clear();
    assert!(builder.build().is_err());
}

#[test]
fn manifest_admission_checks_current_schema_and_immutable_occurrence_roles() {
    let layer: ManifestLayer = serde_json::from_value(manifest_layer()).unwrap();
    let mut builder = RecordingManifestBuilder {
        schema: RecordingManifestSchema::V9,
        dataset_id: super::DATASET.parse().unwrap(),
        recording_segment_id: super::RECORDING.parse().unwrap(),
        catalog_revision: "r1".into(),
        layers: vec![layer.clone()],
        blueprint: None,
        sealed_at: "2026-09-29T10:02:00Z".parse().unwrap(),
    };
    let wire = serde_json::to_value(builder.clone().build().unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<RecordingManifest>(wire.clone()).unwrap())
            .unwrap(),
        wire
    );
    for (field, value) in [
        ("schema", json!("veoveo.ai/recording-manifest/v8")),
        ("layers", json!([])),
        ("sealed_at", json!("yesterday")),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<RecordingManifest>(invalid).is_err(),
            "{field}"
        );
    }
    builder.layers.push(layer.clone());
    assert!(builder.clone().build().is_err());
    let mut distinct: ManifestLayerBuilder = (*layer).clone();
    distinct.layer_id = veoveo_recording_contract::RecordingLayerId::new();
    distinct.layer_name = "derived-sensors".into();
    distinct.kind = veoveo_recording_contract::RecordingLayerKind::Derived;
    distinct.ordinal = None;
    builder.layers[1] = distinct.clone().build().unwrap();
    assert!(
        builder.clone().build().is_err(),
        "same occurrence in two roles"
    );
    distinct.artifact_uri = artifact();
    builder.layers[1] = distinct.build().unwrap();
    assert!(builder.build().is_ok());
    let mut invalid = wire;
    invalid["blueprint"] = json!({"blueprint_id": "blueprint", "revision": 1, "byte_len": 42,
        "message_count": 1, "sha256": "c".repeat(64), "artifact_uri": layer.artifact_uri});
    assert!(serde_json::from_value::<RecordingManifest>(invalid.clone()).is_err());
    invalid["blueprint"]["artifact_uri"] = json!(artifact());
    assert!(serde_json::from_value::<RecordingManifest>(invalid.clone()).is_ok());
    for (field, value) in [
        ("revision", json!(0)),
        ("byte_len", json!(0)),
        ("message_count", json!(0)),
        ("sha256", json!("invalid")),
        ("blueprint_id", json!("")),
    ] {
        let mut bad = invalid.clone();
        bad["blueprint"][field] = value;
        assert!(
            serde_json::from_value::<RecordingManifest>(bad).is_err(),
            "{field}"
        );
    }
}

#[test]
fn checked_metadata_schemas_expose_the_object_and_closed_layer_enums() {
    for schema in [
        schemars::schema_for!(LayerView),
        schemars::schema_for!(ManifestLayer),
        schemars::schema_for!(RecordingView),
        schemars::schema_for!(RecordingManifest),
        schemars::schema_for!(SealRecordingOutput),
    ] {
        let value = serde_json::to_value(schema).unwrap();
        assert_eq!(value["type"], "object");
        assert_eq!(value["additionalProperties"], false);
        assert!(value["properties"].as_object().unwrap().len() > 2);
    }
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(
            veoveo_recording_contract::RecordingLayerKind
        ))
        .unwrap()["enum"],
        json!(["capture", "properties", "derived"])
    );
}
