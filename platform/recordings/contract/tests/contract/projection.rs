use serde_json::json;
use veoveo_recording_contract::{
    CreateRecordingProjectionRequest, CreateRecordingProjectionRequestBuilder,
    MAX_PROJECTION_BYTES, MAX_PROJECTION_COMPONENTS, MAX_PROJECTION_DEADLINE_MS,
    MAX_PROJECTION_ENTITIES, MAX_PROJECTION_ROWS, MAX_PROJECTION_SAMPLES,
    MAX_PROJECTION_SELECTOR_BYTES, RecordingDatasetId, RecordingId, RecordingProjectionQuery,
    RecordingProjectionQueryBuilder, RecordingProjectionSampling, RecordingProjectionSparseFill,
};

fn query_builder() -> RecordingProjectionQueryBuilder {
    RecordingProjectionQueryBuilder {
        entity_paths: vec!["/sensor".into()],
        component_ids: vec!["Scalars:scalars".into()],
        timeline: "tick".into(),
        sampling: RecordingProjectionSampling::SampleGrid { values: vec![2, 4] },
        sparse_fill: RecordingProjectionSparseFill::LatestAtGlobal,
        maximum_entities: 1,
        maximum_columns: 1,
        maximum_samples: 2,
        maximum_rows: 2,
        maximum_bytes: 1024,
    }
}

fn request_builder() -> CreateRecordingProjectionRequestBuilder {
    CreateRecordingProjectionRequestBuilder {
        dataset_id: RecordingDatasetId::new(),
        recording_id: RecordingId::new(),
        query: query_builder().build().unwrap(),
        deadline_ms: 1000,
        idempotency_key: "projection-1".into(),
        units: [("Scalars:scalars".into(), "metres".into())].into(),
        coordinate_frame_refs: Vec::new(),
    }
}

#[test]
fn query_admits_all_sampling_profiles_and_exact_ceilings() {
    let mut builder = query_builder();
    builder.entity_paths = (0..MAX_PROJECTION_ENTITIES)
        .map(|n| format!("/sensor/{n}"))
        .collect();
    builder.component_ids = (0..MAX_PROJECTION_COMPONENTS)
        .map(|n| format!("field{n}"))
        .collect();
    builder.maximum_entities = MAX_PROJECTION_ENTITIES;
    builder.maximum_columns = MAX_PROJECTION_COMPONENTS;
    builder.maximum_samples = MAX_PROJECTION_SAMPLES;
    builder.maximum_rows = MAX_PROJECTION_ROWS;
    builder.maximum_bytes = MAX_PROJECTION_BYTES;
    for sampling in [
        RecordingProjectionSampling::Range {
            start: i64::MIN + 1,
            end: i64::MAX,
        },
        RecordingProjectionSampling::LatestAt { at: i64::MAX },
        RecordingProjectionSampling::SampleGrid {
            values: (0..MAX_PROJECTION_SAMPLES as i64).collect(),
        },
    ] {
        builder.sampling = sampling;
        let query = builder.clone().build().unwrap();
        let wire = serde_json::to_value(&query).unwrap();
        assert_eq!(
            serde_json::from_value::<RecordingProjectionQuery>(wire).unwrap(),
            query
        );
    }
}

#[test]
fn query_rejects_invalid_bounds_selectors_and_sampling_on_decode() {
    let wire = serde_json::to_value(query_builder()).unwrap();
    let mut cases = vec![
        ("entity_paths", json!([])),
        ("entity_paths", json!(["/sensor", "/other"])),
        ("component_ids", json!([])),
        ("component_ids", json!(["Scalars:scalars", "other"])),
        ("entity_paths", json!([""])),
        ("entity_paths", json!(["/private\nname"])),
        ("component_ids", json!([" "])),
        ("timeline", json!("")),
        (
            "timeline",
            json!("x".repeat(MAX_PROJECTION_SELECTOR_BYTES + 1)),
        ),
        ("sampling", json!({"kind": "range", "start": 4, "end": 2})),
        (
            "sampling",
            json!({"kind": "range", "start": i64::MIN, "end": 2}),
        ),
        ("sampling", json!({"kind": "latest_at", "at": i64::MIN})),
        ("sampling", json!({"kind": "sample_grid", "values": []})),
        ("sampling", json!({"kind": "sample_grid", "values": [2, 2]})),
        ("sampling", json!({"kind": "sample_grid", "values": [4, 2]})),
        (
            "sampling",
            json!({"kind": "sample_grid", "values": [1, 2, 3]}),
        ),
        (
            "sampling",
            json!({"kind": "sample_grid", "values": [i64::MIN, 2]}),
        ),
        ("unexpected", json!(true)),
    ];
    for (field, cap) in [
        ("maximum_entities", MAX_PROJECTION_ENTITIES as u64),
        ("maximum_columns", MAX_PROJECTION_COMPONENTS as u64),
        ("maximum_samples", MAX_PROJECTION_SAMPLES as u64),
        ("maximum_rows", MAX_PROJECTION_ROWS),
        ("maximum_bytes", MAX_PROJECTION_BYTES),
    ] {
        cases.extend([(field, json!(0)), (field, json!(cap + 1))]);
    }
    for (field, value) in cases {
        let mut invalid = wire.clone();
        invalid[field] = value;
        assert!(
            serde_json::from_value::<RecordingProjectionQuery>(invalid).is_err(),
            "{field}"
        );
    }
    let mut duplicates = query_builder();
    duplicates.maximum_entities = 2;
    duplicates.entity_paths.push("/sensor".into());
    assert!(duplicates.build().is_err());
    let mut duplicates = query_builder();
    duplicates.maximum_columns = 2;
    duplicates.component_ids.push("Scalars:scalars".into());
    assert!(duplicates.build().is_err());
    let mut zero_bytes = query_builder();
    zero_bytes.maximum_bytes = 0;
    assert!(zero_bytes.build().is_err());
}

#[test]
fn request_builder_and_flat_decoder_share_query_and_metadata_admission() {
    let request = request_builder().build().unwrap();
    let wire = serde_json::to_value(&request).unwrap();
    assert!(wire.get("query").is_none());
    let decoded: CreateRecordingProjectionRequest = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(decoded.query, request.query);
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
    for (field, value) in [
        ("maximum_bytes", json!(0)),
        ("deadline_ms", json!(0)),
        ("deadline_ms", json!(MAX_PROJECTION_DEADLINE_MS + 1)),
        ("idempotency_key", json!(" ")),
        ("idempotency_key", json!("secret\nkey")),
        ("idempotency_key", json!("x".repeat(129))),
        ("units", json!({"unselected-component": "metres"})),
        ("units", json!({"Scalars:scalars": ""})),
        ("units", json!({"Scalars:scalars": "x".repeat(257)})),
        ("coordinate_frame_refs", json!([""])),
        ("coordinate_frame_refs", json!(["private\nframe"])),
        ("coordinate_frame_refs", json!(vec!["frame"; 65])),
        ("query", serde_json::to_value(&request.query).unwrap()),
        ("unbounded", json!(true)),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = value;
        let error =
            serde_json::from_value::<CreateRecordingProjectionRequest>(invalid).unwrap_err();
        assert!(!error.to_string().contains("secret"));
    }
    let mut builder = request_builder();
    builder.deadline_ms = 0;
    assert!(builder.build().is_err());
    let mut builder = request_builder();
    builder
        .units
        .insert("unselected-component".into(), "metres".into());
    assert!(builder.build().is_err());
    let mut builder = request_builder();
    builder.deadline_ms = MAX_PROJECTION_DEADLINE_MS;
    builder.idempotency_key = "x".repeat(128);
    builder.coordinate_frame_refs = vec!["x".repeat(256); 64];
    assert!(builder.build().is_ok());
}

#[test]
fn public_schema_advertises_numeric_bounds_and_closed_flat_shape() {
    let schema =
        serde_json::to_value(schemars::schema_for!(CreateRecordingProjectionRequest)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"].get("query").is_none());
    for (field, maximum) in [
        ("maximum_entities", MAX_PROJECTION_ENTITIES as u64),
        ("maximum_columns", MAX_PROJECTION_COMPONENTS as u64),
        ("maximum_samples", MAX_PROJECTION_SAMPLES as u64),
        ("maximum_rows", MAX_PROJECTION_ROWS),
        ("maximum_bytes", MAX_PROJECTION_BYTES),
        ("deadline_ms", MAX_PROJECTION_DEADLINE_MS),
    ] {
        assert_eq!(schema["properties"][field]["minimum"], json!(1));
        assert_eq!(schema["properties"][field]["maximum"], json!(maximum));
    }
}
