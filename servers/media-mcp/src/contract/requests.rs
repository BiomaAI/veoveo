#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunArgs {
    /// Media model id. Browse media://models or complete the model template.
    pub model: super::MediaModelId,
    /// Model-specific input matching media://model/{+model_id}.
    pub input: serde_json::Map<String, serde_json::Value>,
}
