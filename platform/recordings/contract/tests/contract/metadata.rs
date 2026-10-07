//! Public catalog and manifest admission, independent of Store and MCP runtimes.
use serde_json::{Value, json};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_recording_contract::{
    LayerView, LayerViewBuilder, ManifestLayer, ManifestLayerBuilder, RecordingId,
    RecordingManifest, RecordingManifestBuilder, RecordingManifestSchema, RecordingView,
    SealRecordingOutput, SealRecordingOutputBuilder,
};

fn artifact() -> ArtifactUri {
    ArtifactUri::plane(ArtifactId::new())
}
pub(super) fn layer() -> Value {
    json!({
        "layerId": super::RECORDING, "layerName": "capture-00000000000000000000",
        "kind": "capture", "ordinal": 0, "state": "committed", "byteLen": 1024,
        "messageCount": 3, "sha256": "a".repeat(64), "artifactUri": artifact(),
        "rrdVersion": "0.38.1", "schemaDigest": "b".repeat(64),
        "createdAt": "2026-09-29T10:00:00Z", "updatedAt": "2026-09-29T10:01:00Z"
    })
}
pub(super) fn manifest_layer() -> Value {
    let mut wire = layer();
    for key in ["state", "messageCount", "createdAt", "updatedAt"] {
        wire.as_object_mut().unwrap().remove(key);
    }
    wire
}
pub(super) fn catalog() -> Value {
    json!({
        "recordingId": super::RECORDING, "datasetId": super::DATASET,
        "datasetKey": "recordings", "applicationId": "application", "recordingKey": "capture",
        "state": "sealed", "classification": "unclassified", "labels": ["operations"],
        "startedAt": "2026-09-29T10:00:00Z", "lastDataAt": "2026-09-29T10:01:00Z",
        "endedAt": "2026-09-29T10:01:00Z", "sealedAt": "2026-09-29T10:02:00Z",
        "manifestArtifactUri": ArtifactUri::plane(ArtifactId::try_from(
            RecordingId::parse(super::RECORDING).unwrap().as_uuid()).unwrap()), "layerCount": 1, "committedLayerCount": 1
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
        ("schemaDigest", Value::Null),
        ("rrdVersion", Value::Null),
        ("artifactUri", Value::Null),
        ("artifactUri", json!("artifact://bad")),
        ("messageCount", json!(0)),
        ("byteLen", json!(-1)),
        ("byteLen", json!(0)),
        ("kind", json!("other")),
        ("kind", json!("derived")),
        ("ordinal", json!(1)),
        ("updatedAt", json!("2026-09-29T09:00:00Z")),
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
    staged["artifactUri"] = Value::Null;
    assert!(serde_json::from_value::<LayerView>(staged).is_ok());
    let mut writing = wire;
    writing["state"] = json!("writing");
    for key in ["sha256", "schemaDigest", "rrdVersion", "artifactUri"] {
        writing[key] = Value::Null;
    }
    writing["byteLen"] = json!(0);
    writing["messageCount"] = json!(0);
    assert!(serde_json::from_value::<LayerView>(writing.clone()).is_ok());
    writing["state"] = json!("failed");
    assert!(serde_json::from_value::<LayerView>(writing.clone()).is_ok());
    writing["kind"] = json!("properties");
    writing["layerName"] = json!("properties");
    writing["ordinal"] = Value::Null;
    assert!(serde_json::from_value::<LayerView>(writing.clone()).is_ok());
    writing["kind"] = json!("derived");
    writing["layerName"] = json!("derived-sensors");
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
        ("manifestArtifactUri", json!("artifact://invalid")),
        ("manifestArtifactUri", json!(artifact())),
        ("manifestArtifactUri", Value::Null),
        ("sealedAt", Value::Null),
        ("endedAt", Value::Null),
        ("layerCount", json!(0)),
        ("committedLayerCount", json!(0)),
        ("committedLayerCount", json!(2)),
        ("labels", json!(["operations", "operations"])),
        ("labels", json!(["private", "operations"])),
        ("labels", json!(["x".repeat(257)])),
        ("startedAt", json!("2026-09-29T11:00:00Z")),
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
    for key in ["manifestArtifactUri", "endedAt", "sealedAt"] {
        live[key] = Value::Null;
    }
    live["layerCount"] = json!(0);
    live["committedLayerCount"] = json!(0);
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
        schema: RecordingManifestSchema::V10,
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
        ("schema", json!("veoveo.ai/recording-manifest/v9")),
        ("layers", json!([])),
        ("sealedAt", json!("yesterday")),
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
    invalid["blueprint"] = json!({"blueprintId": "blueprint", "revision": 1, "byteLen": 42,
        "messageCount": 1, "sha256": "c".repeat(64), "artifactUri": layer.artifact_uri});
    assert!(serde_json::from_value::<RecordingManifest>(invalid.clone()).is_err());
    invalid["blueprint"]["artifactUri"] = json!(artifact());
    assert!(serde_json::from_value::<RecordingManifest>(invalid.clone()).is_ok());
    for (field, value) in [
        ("revision", json!(0)),
        ("byteLen", json!(0)),
        ("messageCount", json!(0)),
        ("sha256", json!("invalid")),
        ("blueprintId", json!("")),
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
