//! Run resources bind retained output to the owning Task and pipeline.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{PipelineId, RunId, RunRecordingOutput, RunResultsUri, RunUri, StreamContractError};

#[derive(Clone, Debug)]
pub struct RunDetails {
    pub status: veoveo_task_contract::TaskStatus,
    pub progress: f64,
    pub recording_uri: veoveo_recording_mcp::contract::RecordingUri,
    pub entity_path: String,
    pub timeline: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[serde(try_from = "RunViewWire", into = "RunViewWire")]
pub struct RunView {
    id: RunId,
    pipeline: PipelineId,
    details: RunDetails,
    output: Option<RunRecordingOutput>,
    pub error: Option<String>,
}

impl RunView {
    /// The Task identity cannot be detached from its resource addresses.
    /// ```compile_fail
    /// use veoveo_stream_mcp::contract::{RunView, RunId};
    /// fn detach(view: &mut RunView, id: RunId) { view.task_id = id; }
    /// ```
    pub fn new(id: RunId, pipeline: PipelineId, details: RunDetails) -> Self {
        Self {
            id,
            pipeline,
            details,
            output: None,
            error: None,
        }
    }

    pub fn with_output(
        mut self,
        output: Option<RunRecordingOutput>,
    ) -> Result<Self, StreamContractError> {
        if let Some(output) = &output {
            if output.run_id() != self.id {
                return Err(StreamContractError::InvalidRelationship("run output Task"));
            }
            if output.pipeline_uri.id() != &self.pipeline {
                return Err(StreamContractError::InvalidRelationship(
                    "run output pipeline",
                ));
            }
            let metadata: super::StreamArtifactMetadata =
                serde_json::from_value(output.results_artifact.metadata.clone())
                    .map_err(|_| StreamContractError::InvalidRelationship("run output source"))?;
            match metadata.provenance {
                super::StreamArtifactProvenance::Results { recording_id, .. }
                    if recording_id == self.details.recording_uri.id() => {}
                _ => {
                    return Err(StreamContractError::InvalidRelationship(
                        "run output recording",
                    ));
                }
            }
            if let Some(clip) = &output.source_clip_artifact {
                let metadata: super::StreamArtifactMetadata =
                    serde_json::from_value(clip.metadata.clone())
                        .map_err(|_| StreamContractError::InvalidRelationship("run clip source"))?;
                match metadata.provenance {
                    super::StreamArtifactProvenance::SourceClip {
                        entity_path,
                        timeline,
                        ..
                    } if entity_path == self.details.entity_path
                        && timeline == self.details.timeline => {}
                    _ => {
                        return Err(StreamContractError::InvalidRelationship(
                            "run clip entity and timeline",
                        ));
                    }
                }
            }
        }
        self.output = output;
        Ok(self)
    }

    /// Bind an admitted replay to this Task's selected source and actual Artifact occurrence.
    pub fn check_replay(
        &self,
        selected: &super::RecordingVideoSelection,
        artifact: &veoveo_artifact_contract::ArtifactMetadata,
        results: &super::AnalysisResults,
    ) -> Result<(), StreamContractError> {
        use super::{StreamArtifactMetadata, StreamArtifactProvenance};
        let invalid = || StreamContractError::InvalidRelationship("replay source and Artifact");
        let output = self.output().ok_or_else(invalid)?;
        let owned: StreamArtifactMetadata =
            serde_json::from_value(artifact.metadata.clone()).map_err(|_| invalid())?;
        let StreamArtifactProvenance::Results {
            run_id,
            recording_id,
            pipeline_id,
            model_id,
            source_snapshot_sha256,
        } = owned.provenance
        else {
            return Err(invalid());
        };
        let detection_count = results
            .frames
            .iter()
            .try_fold(0_u64, |count, frame| {
                count.checked_add(frame.detections.len() as u64)
            })
            .ok_or_else(invalid)?;
        if selected.recording_uri != self.details.recording_uri
            || selected.entity_path != self.details.entity_path
            || selected.timeline != self.details.timeline
            || artifact.artifact_uri != output.results_artifact.artifact_uri
            || artifact.metadata != output.results_artifact.metadata
            || run_id != self.task_id()
            || pipeline_id != *self.pipeline_id()
            || pipeline_id != results.pipeline_id
            || model_id != results.model_id
            || model_id != *output.model_uri.id()
            || recording_id != selected.recording_uri.id()
            || results.recording_uri != selected.recording_uri
            || results.entity_path != selected.entity_path
            || results.timeline != selected.timeline
            || results.requested_range != selected.range
            || results
                .source_snapshot
                .digest_sha256()
                .map_err(|_| invalid())?
                != source_snapshot_sha256
            || results.processed_frames != output.summary.processed_frames
            || detection_count != output.summary.detection_count
            || results.elapsed_ms != output.summary.elapsed_ms
            || results.requested_range.start != output.summary.requested_start_index
            || results.requested_range.end != output.summary.requested_end_index
        {
            return Err(invalid());
        }
        Ok(())
    }

    pub fn with_error(mut self, error: Option<String>) -> Self {
        self.error = error;
        self
    }

    pub fn task_id(&self) -> RunId {
        self.id
    }

    pub fn run_uri(&self) -> RunUri {
        RunUri::new(self.id)
    }

    pub fn results_uri(&self) -> RunResultsUri {
        RunResultsUri::new(self.id)
    }

    pub fn pipeline_id(&self) -> &PipelineId {
        &self.pipeline
    }

    pub fn details(&self) -> &RunDetails {
        &self.details
    }

    pub fn output(&self) -> Option<&RunRecordingOutput> {
        self.output.as_ref()
    }
}

// Keep the admitted flat JSON profile while storing each identity only once.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunViewWire {
    run_uri: RunUri,
    results_uri: RunResultsUri,
    task_id: RunId,
    status: veoveo_task_contract::TaskStatus,
    progress: f64,
    pipeline_id: PipelineId,
    recording_uri: veoveo_recording_mcp::contract::RecordingUri,
    entity_path: String,
    timeline: String,
    created_at: String,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<RunRecordingOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl TryFrom<RunViewWire> for RunView {
    type Error = StreamContractError;

    fn try_from(wire: RunViewWire) -> Result<Self, Self::Error> {
        if wire.run_uri.id() != &wire.task_id || wire.results_uri.id() != &wire.task_id {
            return Err(StreamContractError::InvalidRelationship(
                "run Task and URIs",
            ));
        }
        Self::new(
            wire.task_id,
            wire.pipeline_id,
            RunDetails {
                status: wire.status,
                progress: wire.progress,
                recording_uri: wire.recording_uri,
                entity_path: wire.entity_path,
                timeline: wire.timeline,
                created_at: wire.created_at,
                updated_at: wire.updated_at,
            },
        )
        .with_error(wire.error)
        .with_output(wire.output)
    }
}

impl From<RunView> for RunViewWire {
    fn from(view: RunView) -> Self {
        Self {
            run_uri: view.run_uri(),
            results_uri: view.results_uri(),
            task_id: view.id,
            pipeline_id: view.pipeline,
            status: view.details.status,
            progress: view.details.progress,
            recording_uri: view.details.recording_uri,
            entity_path: view.details.entity_path,
            timeline: view.details.timeline,
            created_at: view.details.created_at,
            updated_at: view.details.updated_at,
            output: view.output,
            error: view.error,
        }
    }
}
