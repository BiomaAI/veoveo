use serde_json::{Value, json};
use veoveo_recording_contract::{
    CreateRecordingProjectionRequest, MAX_PROJECTION_BYTES, RECORDING_PROJECTION_HANDLE_SCHEMA,
    RecordingDatasetId, RecordingId, RecordingProjectionHandle, RecordingProjectionHandleBuilder,
    RecordingProjectionHandleSchema, RecordingProjectionId,
};

use super::projection::request_builder;

pub(super) fn wire(request: &CreateRecordingProjectionRequest) -> Value {
    json!({
        "schema": RECORDING_PROJECTION_HANDLE_SCHEMA, "projectionId": RecordingProjectionId::new(),
        "datasetId": request.dataset_id, "recordingId": request.recording_id,
        "result": {"catalogRevision": "catalog-1", "queryDigest": "a".repeat(64),
            "timeline": request.query.timeline, "sampleGrid": request.query.sampling.sample_grid(),
            "units": request.units, "coordinateFrameRefs": request.coordinate_frame_refs,
            "omittedSampleCount": 1, "rowCount": 1, "arrowSchemaSha256": "b".repeat(64),
            "byteLen": 100, "payloadSha256": "c".repeat(64)},
        "expiresAt": "2026-09-29T12:00:00Z"
    })
}

#[test]
fn result_builder_and_decoder_keep_typed_integrity_and_the_wire_profile() {
    let request = request_builder().build().unwrap();
    let wire = wire(&request);
    let builder: RecordingProjectionHandleBuilder = serde_json::from_value(wire.clone()).unwrap();
    let handle = builder.build_for(&request).unwrap();
    assert_eq!(handle.schema, RecordingProjectionHandleSchema::V2);
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
        result["properties"]["payloadSha256"]["pattern"],
        "^[0-9a-f]{64}$"
    );
    assert_eq!(result["properties"]["byteLen"]["minimum"], 1);
}

#[test]
fn result_decode_rejects_invalid_counts_integrity_shapes_and_metadata() {
    let request = request_builder().build().unwrap();
    let wire = wire(&request);
    for (field, value) in [
        ("catalogRevision", json!("")),
        ("timeline", json!("private\nname")),
        ("byteLen", json!(0)),
        ("byteLen", json!(MAX_PROJECTION_BYTES + 1)),
        ("rowCount", json!(10_001)),
        ("rowCount", json!(0)),
        ("omittedSampleCount", json!(u64::MAX)),
        ("sampleGrid", json!([])),
        ("sampleGrid", json!([2, 2])),
        ("sampleGrid", json!([4, 2])),
        ("sampleGrid", json!([i64::MIN, 2])),
        ("queryDigest", json!("a".repeat(63))),
        ("arrowSchemaSha256", json!("B".repeat(64))),
        ("payloadSha256", json!(format!("sha256:{}", "c".repeat(64)))),
        ("units", json!({"component": ""})),
        ("coordinateFrameRefs", json!(vec!["frame"; 65])),
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
        ("schema", json!("veoveo.ai/recording-projection-handle/v1")),
        ("expiresAt", json!("not-a-timestamp")),
        ("extra", json!(true)),
    ] {
        let mut invalid = wire.clone();
        invalid[field] = value;
        assert!(serde_json::from_value::<RecordingProjectionHandle>(invalid).is_err());
    }
    let mut empty_range = wire;
    empty_range["result"]["sampleGrid"] = json!([]);
    empty_range["result"]["omittedSampleCount"] = json!(0);
    empty_range["result"]["rowCount"] = json!(0);
    assert!(serde_json::from_value::<RecordingProjectionHandle>(empty_range).is_ok());
}

#[test]
fn result_agreement_rejects_wrong_parents_selection_metadata_and_request_limits() {
    let request = request_builder().build().unwrap();
    let original = wire(&request);
    for (pointer, value) in [
        ("/datasetId", json!(RecordingDatasetId::new())),
        ("/recordingId", json!(RecordingId::new())),
        ("/result/timeline", json!("other")),
        ("/result/sampleGrid", json!([1, 3])),
        ("/result/units", json!({"Scalars:scalars": "seconds"})),
        (
            "/result/coordinateFrameRefs",
            json!([super::frames::frame(1)]),
        ),
        ("/result/byteLen", json!(1025)),
    ] {
        let mut mismatched = original.clone();
        *mismatched.pointer_mut(pointer).unwrap() = value;
        let handle: RecordingProjectionHandle = serde_json::from_value(mismatched.clone()).unwrap();
        assert!(handle.validate_request(&request).is_err(), "{pointer}");
        let builder: RecordingProjectionHandleBuilder = serde_json::from_value(mismatched).unwrap();
        assert!(builder.build_for(&request).is_err(), "{pointer}");
    }
    let mut limited = serde_json::to_value(&request).unwrap();
    limited["maximumRows"] = json!(1);
    let limited = serde_json::from_value::<CreateRecordingProjectionRequest>(limited).unwrap();
    let mut two_rows = original;
    two_rows["result"]["rowCount"] = json!(2);
    two_rows["result"]["omittedSampleCount"] = json!(0);
    let handle = serde_json::from_value::<RecordingProjectionHandle>(two_rows).unwrap();
    assert!(handle.validate_request(&limited).is_err());

    let mut range = serde_json::to_value(&request).unwrap();
    range["sampling"] = json!({"kind": "range", "start": 0, "end": 2});
    range["maximumSamples"] = json!(1);
    let range = serde_json::from_value::<CreateRecordingProjectionRequest>(range).unwrap();
    let mut result = serde_json::to_value(handle).unwrap();
    result["result"]["sampleGrid"] = json!([]);
    let result = serde_json::from_value::<RecordingProjectionHandle>(result).unwrap();
    assert!(result.validate_request(&range).is_err());
}
