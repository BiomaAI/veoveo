use super::*;
use veoveo_artifact_contract::{
    ArtifactId, ArtifactMetadata, ArtifactReleaseState, ComplianceMetadata,
};
use veoveo_artifact_mcp::contract::ArtifactResource;
use veoveo_reason_mcp::contract::*;

#[derive(Serialize)]
struct InspectionMetadata<'a> {
    summary: &'a str,
    required_action: &'a str,
    location: &'a str,
}

pub(super) fn add(
    builder: &mut Builder,
    s: &Scenario,
    index: usize,
) -> Result<(EvaluationMemberId, EvaluationMemberId, EvaluationMemberId)> {
    let artifact: ArtifactId = format!("01983da0-0000-7000-8000-{:012x}", 100 + index).parse()?;
    let metadata = ArtifactMetadata {
        byte_len: 4096,
        mime_type: Some("application/json".into()),
        filename: Some(format!("{}-inspection.json", s.key)),
        artifact_uri: artifact.plane_uri(),
        download_url: None,
        created_at: observed_at(),
        release_state: ArtifactReleaseState::Private,
        compliance: ComplianceMetadata {
            tenant_id: Some("retrieval-evaluation".parse()?),
            owner: Some(AccessSubject::Principal("evaluator".parse()?)),
            work_context: Some("operations".parse()?),
            ..Default::default()
        },
        metadata: serde_json::to_value(InspectionMetadata {
            summary: &s.finding,
            required_action: &s.action,
            location: &s.place,
        })?,
    };
    let artifact_member = builder.add(
        veoveo_artifact_mcp::knowledge::collection(),
        ArtifactResource::Metadata(artifact).to_uri(),
        &s.title,
        serde_json::to_string(&metadata)?,
        ReadPolicy::SelectedWorkContext {},
    )?;
    let analysis: AnalysisId = format!("01983da0-0000-7000-8000-{:012x}", 200 + index).parse()?;
    let result_artifact: ArtifactId =
        format!("01983da0-0000-7000-8000-{:012x}", 300 + index).parse()?;
    let results = veoveo_reason_mcp::contract::ReasoningResultsBuilder {
        schema: REASONING_RESULTS_SCHEMA.into(),
        pipeline_id: "fixture-video-analysis".parse()?,
        model_id: "fixture-world-model".parse()?,
        recording_uri: "recording://recordings/01983da0-0000-7000-8000-000000000000".parse()?,
        entity_path: "/camera/front".into(),
        timeline: "sensor_time".into(),
        timeline_kind: VideoTimelineKind::DurationNanoseconds,
        requested_range: IndexRange::new(0, 100).unwrap(),
        source_snapshot: serde_json::from_str(include_str!(
            "../../../../../platform/recordings/video/testdata/source-snapshot.json"
        ))?,
        task: ReasoningTask::AnswerQuestion {
            question: s.question.clone(),
        },
        answer: ReasoningAnswer::Answer {
            text: s.finding.clone(),
        },
        observed_frames: 32,
        elapsed_ms: 100,
        prompt_revision: "fictional-evaluation-v1".into(),
        model_digest: None,
        decode: DecodePolicy::Greedy,
        confidence_basis: ConfidenceBasis::ModelReported,
    }
    .build()
    .unwrap();
    let data = FindingData::from_results(&results)?;
    let mut finding = |collection: FindingCollection| -> Result<EvaluationMemberId> {
        let summary = FindingSummary::new(
            collection,
            analysis,
            result_artifact,
            observed_at(),
            observed_at(),
            &data,
        )?;
        builder.add(
            collection.descriptor(),
            summary.uri().to_uri()?,
            &s.title,
            serde_json::to_string(&summary)?,
            ReadPolicy::SelectedWorkContext {},
        )
    };
    Ok((
        artifact_member,
        finding(FindingCollection::Analyses)?,
        finding(FindingCollection::Results)?,
    ))
}
