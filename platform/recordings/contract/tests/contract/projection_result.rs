use serde_json::{Value, json};
use veoveo_recording_contract::{
    CreateRecordingProjectionRequest, MAX_PROJECTION_BYTES, RECORDING_PROJECTION_HANDLE_SCHEMA,
    RecordingDatasetId, RecordingId, RecordingProjectionHandle, RecordingProjectionHandleBuilder,
    RecordingProjectionHandleSchema, RecordingProjectionId,
};

use super::projection::request_builder;

pub(super) fn wire(request: &CreateRecordingProjectionRequest) -> Value {
    json!({
        "schema": RECORDING_PROJECTION_HANDLE_SCHEMA, "projection_id": RecordingProjectionId::new(),
        "dataset_id": request.dataset_id, "recording_id": request.recording_id,
        "result": {"catalog_revision": "catalog-1", "query_digest": "a".repeat(64),
            "timeline": request.query.timeline, "sample_grid": request.query.sampling.sample_grid(),
            "units": request.units, "coordinate_frame_refs": request.coordinate_frame_refs,
            "omitted_sample_count": 1, "row_count": 1, "arrow_schema_sha256": "b".repeat(64),
            "byte_len": 100, "payload_sha256": "c".repeat(64)},
        "expires_at": "2026-09-29T12:00:00Z"
    })
}

#[test]
fn result_builder_and_decoder_keep_typed_integrity_and_the_wire_profile() {
    let request = request_builder().build().unwrap();
    let wire = wire(&request);
    let builder: RecordingProjectionHandleBuilder = serde_json::from_value(wire.clone()).unwrap();
    let handle = builder.build_for(&request).unwrap();
    assert_eq!(handle.schema, RecordingProjectionHandleSchema::V1);
    assert_eq!(handle.result.query_digest.hex(), "a".repeat(64));
    assert_eq!(handle.result.byte_len.get(), 100);
    assert_eq!(serde_json::to_value(&handle).unwrap(), wire);
    let decoded: RecordingProjectionHandle = serde_json::from_value(wire.clone()).unwrap();
    decoded.validate_request(&request).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);
    let schema = serde_json::to_value(schemars::schema_for!(RecordingProjectionHandle)).unwrap();
    assert_eq!(schema["additionalProperties"], false);
    let result = &schema["$defs"]["RecordingProjectionResultMetadata"];
    assert_eq!(result["additionalProperties"], false);
    assert_eq!(
        result["properties"]["payload_sha256"]["pattern"],
        "^[0-9a-f]{64}$"
    );
    assert_eq!(result["properties"]["byte_len"]["minimum"], 1);
}

#[test]
fn result_decode_rejects_invalid_counts_integrity_shapes_and_metadata() {
    let request = request_builder().build().unwrap();
    let wire = wire(&request);
    for (field, value) in [
        ("catalog_revision", json!("")),
        ("timeline", json!("private\nname")),
        ("byte_len", json!(0)),
        ("byte_len", json!(MAX_PROJECTION_BYTES + 1)),
        ("row_count", json!(10_001)),
        ("row_count", json!(0)),
        ("omitted_sample_count", json!(u64::MAX)),
        ("sample_grid", json!([])),
        ("sample_grid", json!([2, 2])),
        ("sample_grid", json!([4, 2])),
        ("sample_grid", json!([i64::MIN, 2])),
        ("query_digest", json!("a".repeat(63))),
        ("arrow_schema_sha256", json!("B".repeat(64))),
        (
            "payload_sha256",
            json!(format!("sha256:{}", "c".repeat(64))),
        ),
        ("units", json!({"component": ""})),
        ("coordinate_frame_refs", json!(vec!["frame"; 65])),
        ("extra", json!(true)),
    ] {
        let mut invalid = wire.clone();
        invalid["result"][field] = value;
        assert!(
            serde_json::from_value::<RecordingProjectionHandle>(invalid).is_err(),
            "{field}"
        );
    }
    for (field, value) in [
        ("schema", json!("veoveo.ai/recording-projection-handle/v2")),
        ("expires_at", json!("not-a-timestamp")),
        ("extra", json!(true)),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<RecordingProjectionHandle>(invalid).is_err());
    }
    let mut empty_range = wire;
    empty_range["result"]["sample_grid"] = json!([]);
    empty_range["result"]["omitted_sample_count"] = json!(0);
    empty_range["result"]["row_count"] = json!(0);
    assert!(serde_json::from_value::<RecordingProjectionHandle>(empty_range).is_ok());
}

#[test]
fn result_agreement_rejects_wrong_parents_selection_metadata_and_request_limits() {
    let request = request_builder().build().unwrap();
    let original = wire(&request);
    for (pointer, value) in [
        ("/dataset_id", json!(RecordingDatasetId::new())),
        ("/recording_id", json!(RecordingId::new())),
        ("/result/timeline", json!("other")),
        ("/result/sample_grid", json!([1, 3])),
        ("/result/units", json!({"Scalars:scalars": "seconds"})),
        (
            "/result/coordinate_frame_refs",
            json!([super::frames::frame(1)]),
        ),
        ("/result/byte_len", json!(1025)),
    ] {
        let mut mismatched = original.clone();
        *mismatched.pointer_mut(pointer).unwrap() = value;
        let handle: RecordingProjectionHandle = serde_json::from_value(mismatched.clone()).unwrap();
        assert!(handle.validate_request(&request).is_err(), "{pointer}");
        let builder: RecordingProjectionHandleBuilder = serde_json::from_value(mismatched).unwrap();
        assert!(builder.build_for(&request).is_err(), "{pointer}");
    }
    let mut limited = serde_json::to_value(&request).unwrap();
    limited["maximum_rows"] = json!(1);
    let limited = serde_json::from_value::<CreateRecordingProjectionRequest>(limited).unwrap();
    let mut two_rows = original;
    two_rows["result"]["row_count"] = json!(2);
    two_rows["result"]["omitted_sample_count"] = json!(0);
    let handle = serde_json::from_value::<RecordingProjectionHandle>(two_rows).unwrap();
    assert!(handle.validate_request(&limited).is_err());

    let mut range = serde_json::to_value(&request).unwrap();
    range["sampling"] = json!({"kind": "range", "start": 0, "end": 2});
    range["maximum_samples"] = json!(1);
    let range = serde_json::from_value::<CreateRecordingProjectionRequest>(range).unwrap();
    let mut result = serde_json::to_value(handle).unwrap();
    result["result"]["sample_grid"] = json!([]);
    let result = serde_json::from_value::<RecordingProjectionHandle>(result).unwrap();
    assert!(result.validate_request(&range).is_err());
}
