//! The same contract checks run in an independent consumer without server features.
#[path = "contract/addresses.rs"]
mod addresses;
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
    assert!(ScopeName::new("independent-server:operate").is_ok());
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(RecordingScope)).unwrap()["enum"],
        json!(["recording:seal"])
    );
}

#[test]
fn catalog_grant_request_preserves_explicit_recording_selection() {
    let wire = json!({"dataset_id": DATASET, "recording_ids": [RECORDING]});
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
        "dataset_id": DATASET, "recording_id": RECORDING,
        "entity_paths": ["/sensor"], "component_ids": ["Scalars:scalars"],
        "timeline": "tick", "sampling": {"kind": "sample_grid", "values": [2, 4]},
        "sparse_fill": "latest_at_global", "maximum_entities": 1, "maximum_columns": 1,
        "maximum_samples": 2, "maximum_rows": 2, "maximum_bytes": 1024,
        "deadline_ms": 1000, "idempotency_key": "projection-1",
        "units": {"Scalars:scalars": "metres"}, "coordinate_frame_refs": []
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
        "schema": RECORDING_CATALOG_GRANT_SCHEMA, "grant_id": DATASET,
        "dataset_id": DATASET, "recording_segment_ids": [RECORDING],
        "catalog_revision": "catalog-1", "entry_uri": veoveo_recording_contract::RecordingCatalogUri::new(
            &veoveo_recording_contract::RecordingRedapOrigin::from_http("http://localhost:8080").unwrap(),
            DATASET.parse().unwrap()),
        "redap_token": "fixture-grant", "expires_at": "2026-09-28T12:00:00Z"
    });
    assert_eq!(
        serde_json::to_value(
            serde_json::from_value::<RecordingCatalogGrant>(grant.clone()).unwrap()
        )
        .unwrap(),
        grant
    );
    let projection = json!({
        "schema": RECORDING_PROJECTION_HANDLE_SCHEMA, "projection_id": DATASET,
        "dataset_id": DATASET, "recording_id": RECORDING,
        "result": {"catalog_revision": "catalog-1", "query_digest": "a".repeat(64),
            "timeline": "tick", "sample_grid": [2, 4], "units": {},
            "coordinate_frame_refs": [], "omitted_sample_count": 0, "row_count": 2,
            "arrow_schema_sha256": "b".repeat(64), "byte_len": 100,
            "payload_sha256": "c".repeat(64)},
        "expires_at": "2026-09-28T12:00:00Z"
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
        assert_eq!(schema["properties"]["dataset_id"]["type"], "string");
    }
}
