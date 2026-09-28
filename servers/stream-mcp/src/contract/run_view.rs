//! Run resources bind retained output to the owning Task and pipeline.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{PipelineId, RunId, RunRecordingOutput, RunResultsUri, RunUri, StreamContractError};

#[derive(Clone, Debug)]
pub struct RunDetails {
    pub status: String,
    pub progress: f64,
    pub recording_uri: String,
    pub entity_path: String,
    pub timeline: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
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
        }
        self.output = output;
        Ok(self)
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
struct RunViewWire {
    run_uri: RunUri,
    results_uri: RunResultsUri,
    task_id: RunId,
    status: String,
    progress: f64,
    pipeline_id: PipelineId,
    recording_uri: String,
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
