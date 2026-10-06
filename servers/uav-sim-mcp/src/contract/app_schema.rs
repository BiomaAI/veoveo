//! Browser bundle roots select existing owner DTOs; these fields are schema tooling,
//! not an additional transport envelope.
use super::*;
use schemars::{JsonSchema, Schema};

#[derive(JsonSchema)]
#[allow(dead_code)]
pub struct AppContracts {
    pub sessions: Vec<SessionSummary>,
    pub cameras: Vec<LiveCameraDescriptor>,
    pub connection: LiveViewConnection,
    pub closed: CloseLiveViewResult,
}

pub fn schema_bundle() -> Schema {
    schemars::schema_for!(AppContracts)
}
