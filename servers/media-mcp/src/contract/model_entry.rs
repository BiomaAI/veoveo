use super::MediaModelId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Model registry value shared by provider admission and public resource consumers.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ModelEntry {
    pub model_id: MediaModelId,
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type", default)]
    pub model_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub base_price: Option<f64>,
    #[serde(default)]
    pub formula: Option<String>,
    #[serde(default)]
    pub api_schema: Option<Value>,
}

impl ModelEntry {
    /// The JSON Schema for this model's run input, if published.
    pub fn request_schema(&self) -> Option<&Value> {
        self.api_schema
            .as_ref()?
            .get("api_schemas")?
            .as_array()?
            .iter()
            .find(|s| s.get("type").and_then(Value::as_str) == Some("model_run"))?
            .get("request_schema")
    }
}
