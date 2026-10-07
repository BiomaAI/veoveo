//! Analysis resources bind retained output to the owning Task and pipeline.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    AnalysisId, AnalysisUri, AnalyzeRecordingOutput, PipelineId, ReasonContractError, ResultsUri,
};

#[derive(Clone, Debug)]
pub struct AnalysisDetails {
    pub status: veoveo_task_contract::TaskStatus,
    pub progress: f64,
    pub task_kind: super::ReasoningKind,
    pub recording_uri: veoveo_recording_mcp::contract::RecordingUri,
    pub entity_path: String,
    pub timeline: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "AnalysisViewWire", into = "AnalysisViewWire")]
pub struct AnalysisView {
    id: AnalysisId,
    pipeline: PipelineId,
    details: AnalysisDetails,
    output: Option<AnalyzeRecordingOutput>,
    pub error: Option<String>,
}

impl AnalysisView {
    pub fn new(id: AnalysisId, pipeline: PipelineId, details: AnalysisDetails) -> Self {
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
        output: Option<AnalyzeRecordingOutput>,
    ) -> Result<Self, ReasonContractError> {
        if let Some(output) = &output {
            if output.analysis_id() != self.id {
                return Err(ReasonContractError::InvalidRelationship(
                    "analysis output Task",
                ));
            }
            if output.pipeline_uri.id() != &self.pipeline {
                return Err(ReasonContractError::InvalidRelationship(
                    "analysis output pipeline",
                ));
            }
            let finding = &output.finding;
            if finding.recording_uri() != &self.details.recording_uri
                || finding.entity_path() != self.details.entity_path
                || finding.timeline() != self.details.timeline
                || super::ReasoningKind::from(finding.task()) != self.details.task_kind
            {
                return Err(ReasonContractError::InvalidRelationship(
                    "analysis output source and kind",
                ));
            }
        }
        self.output = output;
        Ok(self)
    }

    pub fn with_error(mut self, error: Option<String>) -> Self {
        self.error = error;
        self
    }

    pub fn task_id(&self) -> AnalysisId {
        self.id
    }

    pub fn analysis_uri(&self) -> AnalysisUri {
        AnalysisUri::new(self.id)
    }

    pub fn results_uri(&self) -> ResultsUri {
        ResultsUri::new(self.id)
    }

    pub fn pipeline_id(&self) -> &PipelineId {
        &self.pipeline
    }

    pub fn details(&self) -> &AnalysisDetails {
        &self.details
    }

    pub fn output(&self) -> Option<&AnalyzeRecordingOutput> {
        self.output.as_ref()
    }
}

// Keep the admitted flat JSON profile while storing each identity only once.
#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AnalysisViewWire {
    analysis_uri: AnalysisUri,
    results_uri: ResultsUri,
    task_id: AnalysisId,
    status: veoveo_task_contract::TaskStatus,
    progress: f64,
    pipeline_id: PipelineId,
    task_kind: super::ReasoningKind,
    recording_uri: veoveo_recording_mcp::contract::RecordingUri,
    entity_path: String,
    timeline: String,
    created_at: String,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<AnalyzeRecordingOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl TryFrom<AnalysisViewWire> for AnalysisView {
    type Error = ReasonContractError;

    fn try_from(wire: AnalysisViewWire) -> Result<Self, Self::Error> {
        if wire.analysis_uri.id() != &wire.task_id || wire.results_uri.id() != &wire.task_id {
            return Err(ReasonContractError::InvalidRelationship(
                "analysis Task and URIs",
            ));
        }
        Self::new(
            wire.task_id,
            wire.pipeline_id,
            AnalysisDetails {
                status: wire.status,
                progress: wire.progress,
                task_kind: wire.task_kind,
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

impl From<AnalysisView> for AnalysisViewWire {
    fn from(view: AnalysisView) -> Self {
        Self {
            analysis_uri: view.analysis_uri(),
            results_uri: view.results_uri(),
            task_id: view.id,
            pipeline_id: view.pipeline,
            status: view.details.status,
            progress: view.details.progress,
            task_kind: view.details.task_kind,
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
