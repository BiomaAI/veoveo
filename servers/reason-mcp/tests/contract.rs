#[path = "contract/findings.rs"]
mod findings;
#[path = "contract/grounding.rs"]
mod grounding;
#[path = "contract/resources.rs"]
mod resources;
#[path = "contract/responses.rs"]
mod responses;

use veoveo_reason_mcp::contract::*;

#[test]
fn request_defaults_and_validation_are_available_to_contract_consumers() {
    let request: AnalyzeRecordingRequest = serde_json::from_value(serde_json::json!({
        "video": {
            "recording_uri": "recording://recordings/01983da0-0000-7000-8000-000000000000",
            "entity_path": "/camera/front",
            "timeline": "sensor_time",
            "range": {"start": 10, "end": 20}
        },
        "pipeline_id": "traffic-events",
        "task": {"kind": "detect_events", "prompt": "Vehicles entering the intersection"}
    }))
    .unwrap();
    veoveo_recording_video::contract::validate_video_selection(&request.video).unwrap();
    validate_reasoning_task(&request.task).unwrap();
    validate_sampling(request.sampling).unwrap();
    validate_decode(request.decode).unwrap();
    assert_eq!(request.sampling.max_frames, 32);
    assert_eq!(request.decode, DecodePolicy::Greedy);
    assert!(request.grounding.is_none());
    assert!(!request.include_source_clip);
    assert!(validate_sampling(ObservationSampling { max_frames: 0 }).is_err());
    assert!(
        validate_sampling(ObservationSampling {
            max_frames: MAX_OBSERVATION_FRAMES + 1
        })
        .is_err()
    );
    assert!(
        validate_decode(DecodePolicy::Sampled {
            temperature: f32::NAN,
            top_p: 0.5,
            seed: 7
        })
        .is_err()
    );
    let mut wire = serde_json::to_value(&request).unwrap();
    wire["runner_path"] = "/untrusted/runner".into();
    assert!(serde_json::from_value::<AnalyzeRecordingRequest>(wire).is_err());
}

#[test]
fn answer_kinds_and_confidence_provenance_preserve_the_public_wire() {
    for (value, kind, count) in [
        (
            serde_json::json!({"kind": "description", "text": "Road scene"}),
            "describe_segment",
            0,
        ),
        (
            serde_json::json!({"kind": "answer", "text": "Two vehicles"}),
            "answer_question",
            0,
        ),
        (
            serde_json::json!({"kind": "events", "events": [{"range": {"start": 10, "end": 20}, "label": "entry", "description": "Vehicle enters", "track_ids": [7]}]}),
            "detect_events",
            1,
        ),
    ] {
        let answer: ReasoningAnswer = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(answer.kind(), kind);
        assert_eq!(answer.event_count(), count);
        assert_eq!(serde_json::to_value(answer).unwrap(), value);
    }
    assert_eq!(
        serde_json::to_value(ConfidenceBasis::ModelReported).unwrap(),
        "model_reported"
    );
    assert!(serde_json::from_value::<ConfidenceBasis>("calibrated".into()).is_err());
}

#[test]
fn schemas_preserve_the_published_contract() {
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("../testdata/contract.schema.json")).unwrap();
    macro_rules! check { ($($ty:ty),+ $(,)?) => { $(assert_eq!(serde_json::to_value(schemars::schema_for!($ty)).unwrap(), baseline[stringify!($ty)], stringify!($ty));)+ }; }
    check!(
        AnalyzeRecordingRequest,
        ReasoningTask,
        ObservationSampling,
        DecodePolicy,
        GroundingReference,
        GroundingDetections,
        GroundingFrame,
        GroundingDetection,
        ReasoningAnswer,
        ReasonedEvent,
        ConfidenceBasis,
        ReasoningResults,
        AnalyzeRecordingOutput,
        ReasoningSummary,
        PipelineView,
        PipelineOperation,
        ModelView,
        ModelFormat,
        AnalysisView,
        IndexRange,
        RecordingSourceIdentity,
        RecordingSourceIdentityKind,
        RecordingSourceSnapshot,
        RecordingVideoSelection,
        VideoTimelineKind,
    );
}
