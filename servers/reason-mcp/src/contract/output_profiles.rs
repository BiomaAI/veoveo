//! Explicit retained-result compatibility owned by Reason, without storage writes.
use serde::Deserialize;
use veoveo_artifact_contract::ArtifactMetadata;

use super::{
    AnalysisUri, AnalyzeRecordingOutput, ModelUri, PipelineUri, ReasonContractError,
    ReasoningSummary, ResultsUri,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisOutputProfile {
    UnversionedV0,
    V1,
}

#[derive(Debug)]
pub struct RetainedAnalysisOutput {
    profile: AnalysisOutputProfile,
    output: AnalyzeRecordingOutput,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalysisOutputV0 {
    analysis_uri: AnalysisUri,
    results_uri: ResultsUri,
    pipeline_uri: PipelineUri,
    model_uri: ModelUri,
    summary: ReasoningSummary,
    results_artifact: ArtifactMetadata,
    annotations_artifact: ArtifactMetadata,
    source_clip_artifact: Option<ArtifactMetadata>,
}

impl RetainedAnalysisOutput {
    /// Accept only the documented profiles. A declared version never falls back
    /// to the unversioned decoder. The caller separately authorizes the Task.
    pub fn decode(value: serde_json::Value) -> Result<Self, ReasonContractError> {
        if value.get("schema").is_some() {
            return serde_json::from_value(value)
                .map(|output| Self {
                    profile: AnalysisOutputProfile::V1,
                    output,
                })
                .map_err(|_| ReasonContractError::InvalidOutputProfile);
        }
        let legacy: AnalysisOutputV0 =
            serde_json::from_value(value).map_err(|_| ReasonContractError::InvalidOutputProfile)?;
        if legacy.analysis_uri.id() != legacy.results_uri.id() {
            return Err(ReasonContractError::InvalidRelationship(
                "analysis and results URIs",
            ));
        }
        Ok(Self {
            profile: AnalysisOutputProfile::UnversionedV0,
            output: AnalyzeRecordingOutput::new(
                *legacy.analysis_uri.id(),
                legacy.pipeline_uri.id().clone(),
                legacy.model_uri.id().clone(),
                legacy.summary,
                legacy.results_artifact,
                legacy.annotations_artifact,
            )
            .with_source_clip(legacy.source_clip_artifact),
        })
    }

    pub fn profile(&self) -> AnalysisOutputProfile {
        self.profile
    }

    pub fn output(&self) -> &AnalyzeRecordingOutput {
        &self.output
    }

    pub fn into_output(self) -> AnalyzeRecordingOutput {
        self.output
    }
}
