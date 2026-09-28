//! One analysis identity constructs both published analysis and result addresses.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactMetadata;

use super::{
    AnalysisId, AnalysisUri, ModelId, ModelUri, PipelineId, PipelineUri, ReasonContractError,
    ReasoningSummary, ResultsUri,
};

// TODO(foundations): migrate the terminal envelope to the canonical result_uri
// profile, including retained Task results and all installed consumers (C02).
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(
    try_from = "AnalyzeRecordingOutputWire",
    into = "AnalyzeRecordingOutputWire"
)]
pub struct AnalyzeRecordingOutput {
    pub analysis_uri: AnalysisUri,
    pub pipeline_uri: PipelineUri,
    pub model_uri: ModelUri,
    pub summary: ReasoningSummary,
    pub results_artifact: ArtifactMetadata,
    pub annotations_artifact: ArtifactMetadata,
    pub source_clip_artifact: Option<ArtifactMetadata>,
}

impl AnalyzeRecordingOutput {
    pub fn new(
        analysis: AnalysisId,
        pipeline: PipelineId,
        model: ModelId,
        summary: ReasoningSummary,
        results: ArtifactMetadata,
        annotations: ArtifactMetadata,
    ) -> Self {
        Self {
            analysis_uri: AnalysisUri::new(analysis),
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
    pub fn analysis_id(&self) -> AnalysisId {
        *self.analysis_uri.id()
    }
    pub fn results_uri(&self) -> ResultsUri {
        ResultsUri::new(self.analysis_id())
    }
}

#[derive(Clone, Deserialize, Serialize, JsonSchema)]
struct AnalyzeRecordingOutputWire {
    analysis_uri: AnalysisUri,
    results_uri: ResultsUri,
    pipeline_uri: PipelineUri,
    model_uri: ModelUri,
    summary: ReasoningSummary,
    results_artifact: ArtifactMetadata,
    annotations_artifact: ArtifactMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_clip_artifact: Option<ArtifactMetadata>,
}
impl TryFrom<AnalyzeRecordingOutputWire> for AnalyzeRecordingOutput {
    type Error = ReasonContractError;
    fn try_from(wire: AnalyzeRecordingOutputWire) -> Result<Self, Self::Error> {
        if wire.analysis_uri.id() != wire.results_uri.id() {
            return Err(ReasonContractError::InvalidRelationship(
                "analysis and results URIs",
            ));
        }
        Ok(Self::new(
            *wire.analysis_uri.id(),
            wire.pipeline_uri.id().clone(),
            wire.model_uri.id().clone(),
            wire.summary,
            wire.results_artifact,
            wire.annotations_artifact,
        )
        .with_source_clip(wire.source_clip_artifact))
    }
}
impl From<AnalyzeRecordingOutput> for AnalyzeRecordingOutputWire {
    fn from(output: AnalyzeRecordingOutput) -> Self {
        Self {
            results_uri: output.results_uri(),
            analysis_uri: output.analysis_uri,
            pipeline_uri: output.pipeline_uri,
            model_uri: output.model_uri,
            summary: output.summary,
            results_artifact: output.results_artifact,
            annotations_artifact: output.annotations_artifact,
            source_clip_artifact: output.source_clip_artifact,
        }
    }
}
