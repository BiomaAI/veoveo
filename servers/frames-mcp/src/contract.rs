//! Shared Frames domain types below the MCP and Recording runtime dependencies.

pub use veoveo_frames_contract::*;

mod resources;
pub use resources::*;
mod scopes;
pub use scopes::*;

/// A batch publishes an address only when it materializes an Artifact.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(try_from = "BatchTransformTaskWire")]
#[schemars(transform = batch_task_output_schema)]
pub struct BatchTransformTaskOutput(veoveo_types::Checked<BatchTransformTaskWire>);
impl serde::Serialize for BatchTransformTaskOutput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BatchTransformTaskWire {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present_product_uri"
    )]
    #[schemars(with = "veoveo_artifact_contract::ArtifactUri")]
    result_uri: Option<veoveo_artifact_contract::ArtifactUri>,
    #[serde(flatten)]
    output: BatchTransformOutput,
}
impl BatchTransformTaskOutput {
    pub fn new(output: BatchTransformOutput) -> Self {
        Self::try_from(BatchTransformTaskWire {
            result_uri: output.artifact.as_ref().map(|a| a.artifact_uri.clone()),
            output,
        })
        .expect("batch product address derives from its admitted Artifact")
    }
}
impl veoveo_types::Check for BatchTransformTaskWire {
    type Error = &'static str;
    fn check(&self) -> Result<(), Self::Error> {
        if self.result_uri
            != self
                .output
                .artifact
                .as_ref()
                .map(|a| a.artifact_uri.clone())
        {
            return Err("batch product address differs from its Artifact");
        }
        Ok(())
    }
}
impl TryFrom<BatchTransformTaskWire> for BatchTransformTaskOutput {
    type Error = &'static str;
    fn try_from(value: BatchTransformTaskWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
fn deserialize_present_product_uri<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<veoveo_artifact_contract::ArtifactUri>, D::Error> {
    <veoveo_artifact_contract::ArtifactUri as serde::Deserialize>::deserialize(deserializer)
        .map(Some)
}
fn batch_task_output_schema(schema: &mut schemars::Schema) {
    schema.insert(
        "oneOf".into(),
        serde_json::json!([
            {"properties":{"artifact":{"type":"null"}},"not":{"required":["resultUri"]}},
            {"required":["artifact","resultUri"],"properties":{"artifact":{"type":"object"}}}
        ]),
    );
}
