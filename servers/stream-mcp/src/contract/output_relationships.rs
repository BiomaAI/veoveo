//! Relationships visible in the terminal envelope, independent of Artifact access.
use super::{
    RunRecordingOutputBuilder, StreamArtifactMetadata, StreamArtifactProvenance,
    StreamContractError,
};

pub(super) fn check(value: &RunRecordingOutputBuilder) -> Result<(), StreamContractError> {
    let invalid = StreamContractError::InvalidRelationship;
    let results_id = value.results_artifact.artifact_id();
    let annotations_id = value.annotations_artifact.artifact_id();
    if results_id == annotations_id
        || value.source_clip_artifact.as_ref().is_some_and(|clip| {
            clip.artifact_id() == results_id || clip.artifact_id() == annotations_id
        })
    {
        return Err(invalid("Artifact occurrence reused across product roles"));
    }

    if value.run_uri.id() != value.result_uri.id() {
        return Err(invalid("run and results URIs"));
    }
    let summary = &value.summary;
    if summary.requested_start_index > summary.requested_end_index
        || summary.decode_start_index > summary.requested_start_index
    {
        return Err(invalid("recording summary range"));
    }
    let results: StreamArtifactMetadata =
        serde_json::from_value(value.results_artifact.metadata.clone())
            .map_err(|_| invalid("results Artifact provenance"))?;
    let StreamArtifactProvenance::Results {
        run_id,
        recording_id,
        pipeline_id,
        model_id,
        source_snapshot_sha256,
    } = results.provenance
    else {
        return Err(invalid("results Artifact provenance"));
    };
    if run_id != *value.run_uri.id()
        || pipeline_id != *value.pipeline_uri.id()
        || model_id != *value.model_uri.id()
    {
        return Err(invalid("results Artifact identity"));
    }
    let annotations: StreamArtifactMetadata =
        serde_json::from_value(value.annotations_artifact.metadata.clone())
            .map_err(|_| invalid("annotations Artifact provenance"))?;
    match annotations.provenance {
        StreamArtifactProvenance::AnnotationLayer {
            run_id: annotation_run,
            recording_id: annotation_recording,
            results_artifact_uri,
            source_snapshot_sha256: annotation_snapshot,
        } if annotation_run == run_id
            && annotation_recording == recording_id
            && results_artifact_uri.artifact_id() == value.results_artifact.artifact_id()
            && annotation_snapshot == source_snapshot_sha256 => {}
        _ => return Err(invalid("annotations and results Artifact relationship")),
    }
    if let Some(clip) = &value.source_clip_artifact {
        let metadata: StreamArtifactMetadata = serde_json::from_value(clip.metadata.clone())
            .map_err(|_| invalid("source clip Artifact provenance"))?;
        match metadata.provenance {
            StreamArtifactProvenance::SourceClip {
                run_id: clip_run,
                recording_id: clip_recording,
                entity_path,
                timeline,
                decode_start_index,
                source_snapshot_sha256: clip_snapshot,
            } if clip_run == run_id
                && clip_recording == recording_id
                && clip_snapshot == source_snapshot_sha256
                && decode_start_index == summary.decode_start_index =>
            {
                veoveo_recording_video::contract::RecordingVideoSelection::new(
                    veoveo_recording_mcp::contract::RecordingUri::new(recording_id),
                    entity_path,
                    timeline,
                    super::IndexRange::new(
                        summary.requested_start_index,
                        summary.requested_end_index,
                    )
                    .map_err(|_| invalid("source clip range"))?,
                )
                .map_err(|_| invalid("source clip selection"))?;
            }
            _ => return Err(invalid("source clip and results Artifact relationship")),
        }
    }
    Ok(())
}
