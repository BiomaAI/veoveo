//! Browser bundle roots select existing owner DTOs; these fields are schema tooling,
//! not an additional transport envelope.
use super::*;
use schemars::{JsonSchema, Schema};

#[derive(JsonSchema)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct AppContracts {
    pub request: TimeseriesForecastRequest,
    pub forecast: TimeseriesForecastOutput,
}

pub fn schema_bundle() -> Schema {
    schemars::schema_for!(AppContracts)
}
