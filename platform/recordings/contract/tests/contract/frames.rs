use serde_json::json;
use veoveo_frames_contract::{
    FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri, WorldFrameUri,
};
use veoveo_recording_contract::{CreateRecordingProjectionRequest, RecordingProjectionHandle};

pub(super) fn frame(index: usize) -> WorldFrameUri {
    FrameWorldRevisionUri::new(
        &FrameWorldId::parse("survey").unwrap(),
        &FrameWorldRevisionId::parse("revision-1").unwrap(),
    )
    .frame(&FrameId::parse(format!("camera-{index}")).unwrap())
}

#[test]
fn projection_frames_use_the_owner_address_and_preserve_request_identity() {
    let mut builder = super::projection::request_builder();
    builder.coordinate_frame_refs = vec![frame(0), frame(1)];
    let request = builder.build().unwrap();
    let schema =
        serde_json::to_value(schemars::schema_for!(CreateRecordingProjectionRequest)).unwrap();
    assert_eq!(schema["properties"]["coordinateFrameRefs"]["maxItems"], 64);
    let wire = serde_json::to_value(&request).unwrap();
    assert_eq!(
        wire["coordinateFrameRefs"][0],
        "frames://world/survey/revision/revision-1/frame/camera-0"
    );
    let decoded: CreateRecordingProjectionRequest = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(decoded.coordinate_frame_refs, request.coordinate_frame_refs);
    assert_eq!(
        serde_json::to_vec(&decoded.query_identity()).unwrap(),
        serde_json::to_vec(&request.query_identity()).unwrap()
    );
    // The published flat identity uses the URI wire strings in caller order.
    let mut expected_identity = wire.clone();
    expected_identity
        .as_object_mut()
        .unwrap()
        .remove("idempotencyKey");
    assert_eq!(
        serde_json::to_value(request.query_identity()).unwrap(),
        expected_identity
    );
    let result_wire = super::projection_result::wire(&request);
    let result: RecordingProjectionHandle = serde_json::from_value(result_wire.clone()).unwrap();
    result.validate_request(&request).unwrap();
    for frames in [
        json!([frame(0), frame(0)]),
        json!((0..65).map(frame).collect::<Vec<_>>()),
        json!(["frames://world/survey"]),
        json!(["frames://world/survey/revision/revision-1"]),
        json!(["frames://world/survey/revision/revision-1/frame/camera-0?head=latest"]),
        json!(["frames://world/survey/revision/revision-1/frame/%63amera-0"]),
        json!(["frame"]),
    ] {
        let mut invalid = wire.clone();
        invalid["coordinateFrameRefs"] = frames.clone();
        assert!(serde_json::from_value::<CreateRecordingProjectionRequest>(invalid).is_err());
        let mut invalid = result_wire.clone();
        invalid["result"]["coordinateFrameRefs"] = frames;
        assert!(serde_json::from_value::<RecordingProjectionHandle>(invalid).is_err());
    }
    let mut other_revision = result_wire;
    other_revision["result"]["coordinateFrameRefs"][0] =
        json!("frames://world/survey/revision/revision-2/frame/camera-0");
    let other: RecordingProjectionHandle = serde_json::from_value(other_revision).unwrap();
    assert!(other.validate_request(&request).is_err());
}
