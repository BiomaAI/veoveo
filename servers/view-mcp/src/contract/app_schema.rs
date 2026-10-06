//! Browser bundle roots select existing owner DTOs; these fields are schema tooling,
//! not an additional transport envelope.
use super::*;
use schemars::{JsonSchema, Schema};

#[derive(JsonSchema)]
#[allow(dead_code)]
pub struct AppContracts {
    pub layers: Vec<LayerSummary>,
    pub composition: SceneComposition,
    pub view: ViewRecord,
    pub scene: PreviewSceneRecord,
    pub frame: FrameRecord,
    pub closed: CloseViewResult,
}

pub fn schema_bundle() -> Schema {
    schemars::schema_for!(AppContracts)
}
