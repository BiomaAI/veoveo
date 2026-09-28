//! A recording run constructs both its run and result addresses.
use super::{
    AnalysisSummary, ModelId, ModelUri, PipelineId, PipelineUri, RunId, RunResultsUri, RunUri,
    StreamContractError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactMetadata;

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(try_from = "RunRecordingOutputWire", into = "RunRecordingOutputWire")]
pub struct RunRecordingOutput {
    pub run_uri: RunUri,
    pub pipeline_uri: PipelineUri,
    pub model_uri: ModelUri,
    pub summary: AnalysisSummary,
    pub results_artifact: ArtifactMetadata,
    pub annotations_artifact: ArtifactMetadata,
    pub source_clip_artifact: Option<ArtifactMetadata>,
}
impl RunRecordingOutput {
    pub fn new(
        run: RunId,
        pipeline: PipelineId,
        model: ModelId,
        summary: AnalysisSummary,
        results: ArtifactMetadata,
        annotations: ArtifactMetadata,
    ) -> Self {
        Self {
            run_uri: RunUri::new(run),
            pipeline_uri: PipelineUri::new(pipeline),
            model_uri: ModelUri::new(model),
            summary,
            results_artifact: results,
            annotations_artifact: annotations,
            source_clip_artifact: None,
        }
    }
    pub fn with_source_clip(mut self, clip: Option<ArtifactMetadata>) -> Self {
        self.source_clip_artifact = clip;
        self
    }
    pub fn run_id(&self) -> RunId {
        *self.run_uri.id()
    }
    pub fn result_uri(&self) -> RunResultsUri {
        RunResultsUri::new(self.run_id())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
struct RunRecordingOutputWire {
    run_uri: RunUri,
    result_uri: RunResultsUri,
    pipeline_uri: PipelineUri,
    model_uri: ModelUri,
    summary: AnalysisSummary,
    results_artifact: ArtifactMetadata,
    annotations_artifact: ArtifactMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_clip_artifact: Option<ArtifactMetadata>,
}
impl TryFrom<RunRecordingOutputWire> for RunRecordingOutput {
    type Error = StreamContractError;
    fn try_from(wire: RunRecordingOutputWire) -> Result<Self, Self::Error> {
        if wire.run_uri.id() != wire.result_uri.id() {
            return Err(StreamContractError::InvalidRelationship(
                "run and results URIs",
            ));
        }
        Ok(Self::new(
            *wire.run_uri.id(),
            wire.pipeline_uri.id().clone(),
            wire.model_uri.id().clone(),
            wire.summary,
            wire.results_artifact,
            wire.annotations_artifact,
        )
        .with_source_clip(wire.source_clip_artifact))
    }
}
impl From<RunRecordingOutput> for RunRecordingOutputWire {
    fn from(view: RunRecordingOutput) -> Self {
        Self {
            result_uri: view.result_uri(),
            run_uri: view.run_uri,
            pipeline_uri: view.pipeline_uri,
            model_uri: view.model_uri,
            summary: view.summary,
            results_artifact: view.results_artifact,
            annotations_artifact: view.annotations_artifact,
            source_clip_artifact: view.source_clip_artifact,
        }
    }
}
