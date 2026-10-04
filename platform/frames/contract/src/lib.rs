mod task_kind;
pub use task_kind::FramesTaskKind;
mod catalog;
mod ids;
mod metadata;
mod operations;
mod streams;
mod tree;
mod uris;
mod usage;
mod world;

pub use catalog::*;
pub use ids::*;
pub use metadata::*;
pub use operations::*;
pub use streams::*;
pub use tree::*;
pub use uris::*;
pub use usage::*;
pub use world::*;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactMetadata;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EcefPosition {
    pub x_m: f64,
    pub y_m: f64,
    pub z_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldFramePosition {
    pub frame_uri: WorldFrameUri,
    pub x_m: f64,
    pub y_m: f64,
    pub z_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoordinatePoint {
    Wgs84(Wgs84Position),
    EcefWgs84(EcefPosition),
    WorldFrame(WorldFramePosition),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConvertFrameRequest {
    pub target: CoordinateSpace,
    #[schemars(length(min = 1, max = 10_000))]
    pub points: Vec<CoordinatePoint>,
    #[serde(default)]
    pub allow_approximation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConvertFrameOutput {
    pub points: Vec<CoordinatePoint>,
    pub provenance: CoordinateOperationProvenance,
    pub sources: Vec<FrameSourceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateWorldRequest {
    pub world_id: FrameWorldId,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateWorldOutput {
    pub world: FrameWorldSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublishWorldRequest {
    pub world_id: FrameWorldId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_head_revision_id: Option<FrameWorldRevisionId>,
    pub tree: FrameWorldTree,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PublishWorldOutput {
    pub world: FrameWorldSummary,
    pub revision: FrameWorldRevision,
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchTransformRequest {
    pub convert: ConvertFrameRequest,
    #[serde(default)]
    pub artifact: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchTransformOutput {
    pub result: ConvertFrameOutput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactMetadata>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_frame_position_round_trips_with_typed_uri() {
        let revision = crate::FrameWorldRevisionUri::new(
            &FrameWorldId::parse("uav-showcase-new-york").unwrap(),
            &FrameWorldRevisionId::parse("revision-1").unwrap(),
        );
        let point = CoordinatePoint::WorldFrame(WorldFramePosition {
            frame_uri: WorldFrameUri::new(
                &revision,
                &crate::FrameId::parse("isaac-world").unwrap(),
            ),
            x_m: 1.0,
            y_m: 2.0,
            z_m: 3.0,
        });
        assert_eq!(
            serde_json::from_value::<CoordinatePoint>(serde_json::to_value(&point).unwrap())
                .unwrap(),
            point
        );
    }

    #[test]
    fn conversion_output_schema_requires_compiler_ready_sources() {
        let schema = serde_json::to_value(schemars::schema_for!(ConvertFrameOutput)).unwrap();
        let properties = schema["properties"].as_object().unwrap();

        assert!(properties.contains_key("sources"));
    }
}
