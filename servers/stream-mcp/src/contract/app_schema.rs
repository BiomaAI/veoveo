//! Browser bundle roots select existing owner DTOs; these fields are schema tooling,
//! not an additional transport envelope.
use super::*;
use schemars::{JsonSchema, Schema};

#[derive(JsonSchema)]
#[allow(dead_code)]
pub struct AppContracts {
    pub pipelines: Vec<PipelineView>,
    pub sessions: LiveSessionsPage,
    pub session: LiveSessionView,
    pub results: LiveResultsView,
    pub preview: LivePreviewView,
    pub started: StartLiveSessionOutput,
    pub stopped: StopLiveSessionOutput,
}

pub fn schema_bundle() -> Schema {
    schemars::schema_for!(AppContracts)
}
