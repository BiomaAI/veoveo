use super::{MediaModelId, MediaModelUri};
use serde_json::Value;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelsArgs {
    /// Case-insensitive search over model id, name, type, and description.
    #[serde(default)]
    pub query: Option<String>,
    /// Exact model type filter, e.g. text-to-image, image-to-image, text-to-video.
    #[serde(default, rename = "type")]
    pub model_type: Option<String>,
    /// Maximum number of models to return. Defaults to 20 and cannot exceed 100.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelSchemaArgs {
    /// Exact model id, e.g. wavespeed-ai/flux-schnell.
    pub model: MediaModelId,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ModelCatalogItem {
    pub model_id: MediaModelId,
    pub name: String,
    #[serde(rename = "type")]
    pub model_type: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_price: Option<f64>,
    pub schema_uri: MediaModelUri,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ModelCatalogOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub model_type: Option<String>,
    pub total_available: usize,
    pub returned: usize,
    pub models: Vec<ModelCatalogItem>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ModelSchemaOutput {
    pub model_id: MediaModelId,
    pub name: String,
    #[serde(rename = "type")]
    pub model_type: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formula: Option<String>,
    pub schema_uri: MediaModelUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_schema: Option<Value>,
}
