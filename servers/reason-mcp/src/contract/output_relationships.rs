//! Terminal relationships that the published finding and descriptors can prove.
use super::{
    AnalyzeRecordingOutputBuilder, FindingAnswer, ReasonArtifactMetadata, ReasonArtifactProvenance,
    ReasonContractError, ReasoningKind,
};

pub(super) fn check(value: &AnalyzeRecordingOutputBuilder) -> Result<(), ReasonContractError> {
    let invalid = ReasonContractError::InvalidRelationship;
    let results_id = value.results_artifact.artifact_id();
    let annotations_id = value.annotations_artifact.artifact_id();
    if results_id == annotations_id
        || value.source_clip_artifact.as_ref().is_some_and(|clip| {
            clip.artifact_id() == results_id || clip.artifact_id() == annotations_id
        })
    {
        return Err(invalid("Artifact occurrence reused across product roles"));
    }

    if value.analysis_uri.id() != value.result_uri.id() {
        return Err(invalid("analysis and results URIs"));
    }
    let finding = &value.finding;
    if value.pipeline_uri.id() != finding.pipeline_id()
        || value.model_uri.id() != finding.model_id()
    {
        return Err(invalid("finding pipeline/model identity"));
    }
    let expected_events = match finding.answer() {
        FindingAnswer::Events { total, .. } => *total,
        _ => 0,
    };
    if value.summary.observed_frames == 0
        || value.summary.requested_start_index != finding.requested_range().start
        || value.summary.requested_end_index != finding.requested_range().end
        || value.summary.decode_start_index > value.summary.requested_start_index
        || value.summary.event_count != expected_events
    {
        return Err(invalid("finding and summary relationship"));
    }
    let metadata: ReasonArtifactMetadata =
        serde_json::from_value(value.results_artifact.metadata.clone())
            .map_err(|_| invalid("results Artifact provenance"))?;
    match metadata.provenance {
        ReasonArtifactProvenance::Results {
            analysis_id,
            recording_id,
            pipeline_id,
            model_id,
            prompt_revision,
            task_kind,
            source_snapshot_sha256,
        } if analysis_id == *value.analysis_uri.id()
            && recording_id == finding.recording_uri().id()
            && pipeline_id == *finding.pipeline_id()
            && model_id == *finding.model_id()
            && prompt_revision == finding.prompt_revision()
            && task_kind == ReasoningKind::from(finding.task())
            && &source_snapshot_sha256 == finding.source_snapshot_sha256() => {}
        _ => return Err(invalid("finding and results Artifact provenance")),
    }
    let annotations: ReasonArtifactMetadata =
        serde_json::from_value(value.annotations_artifact.metadata.clone())
            .map_err(|_| invalid("annotations Artifact provenance"))?;
    match annotations.provenance {
        ReasonArtifactProvenance::AnnotationLayer {
            analysis_id,
            recording_id,
            results_artifact_uri,
            source_snapshot_sha256,
        } if analysis_id == *value.analysis_uri.id()
            && recording_id == finding.recording_uri().id()
            && results_artifact_uri.artifact_id() == value.results_artifact.artifact_id()
            && &source_snapshot_sha256 == finding.source_snapshot_sha256() => {}
        _ => return Err(invalid("annotations and results Artifact relationship")),
    }
    if let Some(clip) = &value.source_clip_artifact {
        let metadata: ReasonArtifactMetadata = serde_json::from_value(clip.metadata.clone())
            .map_err(|_| invalid("source clip Artifact provenance"))?;
        match metadata.provenance {
            ReasonArtifactProvenance::SourceClip {
                analysis_id,
                recording_id,
                entity_path,
                timeline,
                decode_start_index,
                source_snapshot_sha256,
            } if analysis_id == *value.analysis_uri.id()
                && recording_id == finding.recording_uri().id()
                && entity_path == finding.entity_path()
                && timeline == finding.timeline()
                && decode_start_index == value.summary.decode_start_index
                && &source_snapshot_sha256 == finding.source_snapshot_sha256() => {}
            _ => return Err(invalid("source clip and finding relationship")),
        }
    }
    Ok(())
}
