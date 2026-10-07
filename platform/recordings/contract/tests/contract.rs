//! The same contract checks run in an independent consumer without server features.
#[path = "contract/addresses.rs"]
mod addresses;
#[path = "contract/frames.rs"]
mod frames;
#[path = "contract/metadata.rs"]
mod metadata;
#[path = "contract/playback.rs"]
mod playback;
#[path = "contract/projection.rs"]
mod projection;
#[path = "contract/projection_result.rs"]
mod projection_result;
#[path = "contract/redap.rs"]
mod redap;

use serde_json::{Value, json};
use veoveo_recording_contract::{
    CreateRecordingCatalogGrantRequest, CreateRecordingProjectionRequest,
    RECORDING_CATALOG_GRANT_SCHEMA, RECORDING_PROJECTION_HANDLE_SCHEMA, RecordingCatalogGrant,
    RecordingProjectionHandle, RecordingProjectionSampling, RecordingProjectionSparseFill,
};

const DATASET: &str = "0197f78e-f2f0-7a6e-8a5d-f41c691e4471";
const RECORDING: &str = "0197f78e-f2f0-7a6e-8a5d-f41c691e4472";

#[test]
fn producer_permissions_are_distinct_from_sealing_and_match_the_wire_profile() {
    use veoveo_recording_contract::{RecordingProducerScope, RecordingScope};
    use veoveo_types::ScopeDefinition;
    for (scope, name) in [
        (RecordingProducerScope::Ingest, "recording:ingest"),
        (RecordingProducerScope::Publish, "recording:publish"),
    ] {
        assert_eq!(scope.name().as_str(), name);
        assert_eq!(name.parse(), Ok(scope));
        assert_eq!(serde_json::to_value(scope).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<RecordingProducerScope>(json!(name)).unwrap(),
            scope
        );
        assert!(RecordingScope::try_from(scope.name()).is_err());
    }
    for name in [
        "recording:seal",
        "admin:manage",
        "recording:Ingest",
        "recording:ingest recording:publish",
    ] {
        assert!(name.parse::<RecordingProducerScope>().is_err());
    }
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(RecordingProducerScope)).unwrap()["enum"],
        json!(["recording:ingest", "recording:publish"])
    );
}

#[test]
fn sealing_permission_is_an_owner_type_with_one_wire_spelling() {
    use veoveo_recording_contract::RecordingScope;
    use veoveo_types::{ScopeDefinition, ScopeName};
    assert_eq!(
        serde_json::to_value(RecordingScope::Seal).unwrap(),
        json!("recording:seal")
    );
    assert_eq!(
        serde_json::from_value::<RecordingScope>(json!("recording:seal")).unwrap(),
        RecordingScope::Seal
    );
    assert_eq!(
        RecordingScope::try_from(RecordingScope::Seal.name()).unwrap(),
        RecordingScope::Seal
    );
    for value in [
        "admin:manage",
        "recording:ingest",
        "recording:Seal",
        "recording:seal extra",
    ] {
        assert!(serde_json::from_value::<RecordingScope>(json!(value)).is_err());
    }
    assert!(ScopeName::parse("independent-server:operate").is_ok());
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(RecordingScope)).unwrap()["enum"],
        json!(["recording:seal"])
    );
}

#[test]
fn catalog_grant_request_preserves_explicit_recording_selection() {
    let wire = json!({"datasetId": DATASET, "recordingIds": [RECORDING]});
    let request: CreateRecordingCatalogGrantRequest = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(request.dataset_id().to_string(), DATASET);
    assert_eq!(request.recording_ids()[0].to_string(), RECORDING);
    assert_eq!(serde_json::to_value(request).unwrap(), wire);
    let mut extra = wire;
    extra["admit_all"] = json!(true);
    assert!(serde_json::from_value::<CreateRecordingCatalogGrantRequest>(extra).is_err());
}

#[test]
fn projection_request_preserves_selectors_bounds_and_sampling() {
    let wire = json!({
        "datasetId": DATASET, "recordingId": RECORDING,
        "entityPaths": ["/sensor"], "componentIds": ["Scalars:scalars"],
        "timeline": "tick", "sampling": {"kind": "sample_grid", "values": [2, 4]},
        "sparseFill": "latest_at_global", "maximumEntities": 1, "maximumColumns": 1,
        "maximumSamples": 2, "maximumRows": 2, "maximumBytes": 1024,
        "deadlineMs": 1000, "idempotencyKey": "projection-1",
        "units": {"Scalars:scalars": "metres"}, "coordinateFrameRefs": []
    });
    let request: CreateRecordingProjectionRequest = serde_json::from_value(wire.clone()).unwrap();
    assert!(
        matches!(request.query.sampling, RecordingProjectionSampling::SampleGrid { ref values } if values == &[2, 4])
    );
    assert!(matches!(
        request.query.sparse_fill,
        RecordingProjectionSparseFill::LatestAtGlobal
    ));
    assert_eq!(serde_json::to_value(request).unwrap(), wire);
    for sampling in [
        json!({"kind": "unbounded"}),
        json!({"kind": "range", "start": 0}),
        json!({"kind": "latest_at", "at": 2, "extra": true}),
    ] {
        let mut invalid = wire.clone();
        invalid["sampling"] = sampling;
        assert!(serde_json::from_value::<CreateRecordingProjectionRequest>(invalid).is_err());
    }
}

#[test]
fn response_models_preserve_grant_and_projection_wire_shapes() {
    let grant = json!({
        "schema": RECORDING_CATALOG_GRANT_SCHEMA, "grantId": DATASET,
        "datasetId": DATASET, "recordingSegmentIds": [RECORDING],
        "catalogRevision": "catalog-1", "entryUri": veoveo_recording_contract::RecordingCatalogUri::new(
            &veoveo_recording_contract::RecordingRedapOrigin::from_http("http://localhost:8080").unwrap(),
            DATASET.parse().unwrap()),
        "redapToken": "fixture-grant", "expiresAt": "2026-09-28T12:00:00Z"
    });
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<RecordingCatalogGrant>(grant.clone()).unwrap()
        )
        .unwrap(),
        grant
    );
    let projection = json!({
        "schema": RECORDING_PROJECTION_HANDLE_SCHEMA, "projectionId": DATASET,
        "datasetId": DATASET, "recordingId": RECORDING,
        "result": {"catalogRevision": "catalog-1", "queryDigest": "a".repeat(64),
            "timeline": "tick", "sampleGrid": [2, 4], "units": {},
            "coordinateFrameRefs": [], "omittedSampleCount": 0, "rowCount": 2,
            "arrowSchemaSha256": "b".repeat(64), "byteLen": 100,
            "payloadSha256": "c".repeat(64)},
        "expiresAt": "2026-09-28T12:00:00Z"
    });
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<RecordingProjectionHandle>(projection.clone()).unwrap()
        )
        .unwrap(),
        projection
    );
}

#[test]
fn contract_schemas_are_available_without_protocol_or_runtime_types() {
    for schema in [
        schemars::schema_for!(CreateRecordingCatalogGrantRequest),
        schemars::schema_for!(CreateRecordingProjectionRequest),
        schemars::schema_for!(RecordingCatalogGrant),
        schemars::schema_for!(RecordingProjectionHandle),
    ] {
        let schema: Value = serde_json::to_value(schema).unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["datasetId"]["type"], "string");
    }
}

#[test]
fn checked_builder_schema_profiles_are_preserved() {
    use schemars::JsonSchema;
    use veoveo_recording_contract::{
        LayerView, LayerViewBuilder, ManifestLayer, ManifestLayerBuilder, RecordingManifest,
        RecordingManifestBuilder, RecordingView, RecordingViewBuilder, SealRecordingOutput,
        SealRecordingOutputBuilder,
    };
    fn same<Model: JsonSchema, Builder: JsonSchema>() {
        assert_eq!(Model::schema_name(), Builder::schema_name());
        assert_eq!(Model::schema_id(), Builder::schema_id());
        assert_eq!(Model::inline_schema(), Builder::inline_schema());
        assert_eq!(schemars::schema_for!(Model), schemars::schema_for!(Builder));
    }
    same::<LayerView, LayerViewBuilder>();
    same::<ManifestLayer, ManifestLayerBuilder>();
    same::<RecordingView, RecordingViewBuilder>();
    same::<SealRecordingOutput, SealRecordingOutputBuilder>();
    same::<RecordingManifest, RecordingManifestBuilder>();
}

/// Corrupt an existing admitted member, including optional and nested occurrences.
fn refuse_retired_members<T: serde::de::DeserializeOwned>(wire: Value, dictionaries: &[&str]) {
    serde_json::from_value::<T>(wire.clone()).unwrap();
    fn collect(
        value: &Value,
        path: &str,
        dictionaries: &[&str],
        cases: &mut Vec<(String, String, String)>,
    ) {
        // Owner-declared dictionary keys belong to the source, not the DTO vocabulary.
        if dictionaries.contains(&path) {
            assert!(value.is_object(), "{path}: dictionary must be an object");
            return;
        }
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    let retired: String = key
                        .chars()
                        .flat_map(|ch| {
                            if ch.is_ascii_uppercase() {
                                vec!['_', ch.to_ascii_lowercase()]
                            } else {
                                vec![ch]
                            }
                        })
                        .collect();
                    if retired != *key {
                        cases.push((path.into(), key.clone(), retired));
                    }
                    collect(
                        child,
                        &format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                        dictionaries,
                        cases,
                    );
                }
            }
            Value::Array(array) => {
                for (index, child) in array.iter().enumerate() {
                    collect(child, &format!("{path}/{index}"), dictionaries, cases);
                }
            }
            _ => {}
        }
    }
    let mut cases = Vec::new();
    collect(&wire, "", dictionaries, &mut cases);
    assert!(!cases.is_empty());
    for (pointer, current, retired) in cases {
        for mode in ["replacement", "mixed", "conflicting"] {
            let mut invalid = wire.clone();
            let object = invalid
                .pointer_mut(&pointer)
                .unwrap()
                .as_object_mut()
                .unwrap();
            let value = object.get(&current).unwrap().clone();
            object.insert(
                retired.clone(),
                if mode == "conflicting" {
                    json!("retired-conflict")
                } else {
                    value
                },
            );
            if mode == "replacement" {
                object.remove(&current).unwrap();
            }
            assert!(
                serde_json::from_value::<T>(invalid).is_err(),
                "{pointer}/{current}/{mode}"
            );
        }
    }
}

#[test]
fn current_controlled_members_admit_one_spelling_at_every_occurrence() {
    use veoveo_recording_contract::*;
    refuse_retired_members::<PlaybackManifest>(playback::manifest(), &[]);
    refuse_retired_members::<RecordingCatalogGrant>(
        serde_json::to_value(redap::grant_builder().build().unwrap()).unwrap(),
        &[],
    );
    let request = projection::request_builder().build().unwrap();
    refuse_retired_members::<CreateRecordingProjectionRequest>(
        serde_json::to_value(&request).unwrap(),
        &["/units"],
    );
    refuse_retired_members::<RecordingProjectionHandle>(
        projection_result::wire(&request),
        &["/result/units"],
    );
    // Component identities preserve their source spelling even when they resemble DTO members.
    for key in [
        "Scalars:scalars",
        "scalars:scalars",
        "sensor/front~Scalars:scalars",
    ] {
        let mut wire = serde_json::to_value(&request).unwrap();
        wire["componentIds"] = json!([key]);
        wire["units"] = json!({(key): "metres"});
        let selected: CreateRecordingProjectionRequest =
            serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(&selected).unwrap(), wire);
        let result_wire = projection_result::wire(&selected);
        let handle: RecordingProjectionHandle =
            serde_json::from_value(result_wire.clone()).unwrap();
        handle.validate_request(&selected).unwrap();
        assert_eq!(serde_json::to_value(handle).unwrap(), result_wire);
        for value in [json!(null), json!(17), json!({}), json!([])] {
            let mut invalid = wire.clone();
            invalid["units"][key] = value.clone();
            assert!(serde_json::from_value::<CreateRecordingProjectionRequest>(invalid).is_err());
            let mut invalid = result_wire.clone();
            invalid["result"]["units"][key] = value;
            assert!(serde_json::from_value::<RecordingProjectionHandle>(invalid).is_err());
        }
    }
    refuse_retired_members::<LayerView>(metadata::layer(), &[]);
    refuse_retired_members::<RecordingView>(metadata::catalog(), &[]);
    let manifest = RecordingManifestBuilder {
        schema: RecordingManifestSchema::V10,
        dataset_id: DATASET.parse().unwrap(),
        recording_segment_id: RECORDING.parse().unwrap(),
        catalog_revision: "r1".into(),
        layers: vec![serde_json::from_value(metadata::manifest_layer()).unwrap()],
        blueprint: None,
        sealed_at: "2026-09-29T10:02:00Z".parse().unwrap(),
    }
    .build()
    .unwrap();
    refuse_retired_members::<RecordingManifest>(serde_json::to_value(manifest).unwrap(), &[]);
    refuse_retired_members::<SealRecordingRequest>(json!({"recordingId":RECORDING}), &[]);
    refuse_retired_members::<CreateRecordingCatalogGrantRequest>(
        json!({"datasetId":DATASET,"recordingIds":[RECORDING]}),
        &[],
    );
    let capture = RecordingCaptureMetadata {
        recording_id: RECORDING.parse().unwrap(),
        dataset_id: DATASET.parse().unwrap(),
        layer_kind: RecordingLayerKind::Capture,
        schema_digest: veoveo_types::Sha256Digest::from_bytes([7; 32]),
    };
    refuse_retired_members::<RecordingCaptureMetadata>(serde_json::to_value(capture).unwrap(), &[]);
    let artifact = RecordingArtifactMetadata {
        provenance: RecordingArtifactProvenance::RecordingManifest {
            recording_id: RECORDING.parse().unwrap(),
            dataset_id: DATASET.parse().unwrap(),
            catalog_revision: "r1".into(),
            dataset_revision: 1,
            recording_revision: 1,
            sealed_at: "2026-09-29T10:02:00Z".parse().unwrap(),
            sha256: veoveo_types::Sha256Digest::from_bytes([8; 32]),
        },
    };
    refuse_retired_members::<RecordingArtifactMetadata>(
        serde_json::to_value(artifact).unwrap(),
        &[],
    );
}
