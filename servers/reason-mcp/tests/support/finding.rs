use veoveo_reason_mcp::contract::*;

pub fn results() -> ReasoningResults {
    veoveo_reason_mcp::contract::ReasoningResultsBuilder {
        schema: "veoveo.ai/reason-results/v2".into(),
        pipeline_id: "video-reasoning".parse().unwrap(),
        model_id: "world-model".parse().unwrap(),
        recording_uri: "recording://recordings/01983da0-0000-7000-8000-000000000000"
            .parse()
            .unwrap(),
        entity_path: "/camera/front".into(),
        timeline: "sensor_time".into(),
        timeline_kind: VideoTimelineKind::DurationNanoseconds,
        requested_range: IndexRange::new(0, 100).unwrap(),
        source_snapshot: serde_json::from_str(include_str!(
            "../../../../platform/recordings/video/testdata/source-snapshot.json"
        ))
        .unwrap(),
        task: ReasoningTask::AnswerQuestion {
            question: "How many vehicles entered?".into(),
        },
        answer: ReasoningAnswer::Answer {
            text: "Two vehicles entered.".into(),
        },
        observed_frames: 32,
        elapsed_ms: 100,
        prompt_revision: "traffic-v1".into(),
        model_digest: Some(
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
        ),
        decode: DecodePolicy::Greedy,
        confidence_basis: ConfidenceBasis::ModelReported,
    }
    .build()
    .unwrap()
}
