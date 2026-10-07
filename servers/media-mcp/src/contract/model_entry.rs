use super::MediaModelId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Provider-native registry value; its serialized bytes define the registry digest.
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

/// Complete owned resource view; the provider schema envelope stays open.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelResourceOutput {
    pub model_id: MediaModelId,
    pub name: String,
    #[serde(rename = "type")]
    pub model_type: String,
    pub description: String,
    pub base_price: Option<f64>,
    pub formula: Option<String>,
    pub api_schema: Option<Value>,
}

impl From<ModelEntry> for ModelResourceOutput {
    fn from(entry: ModelEntry) -> Self {
        Self {
            model_id: entry.model_id,
            name: entry.name,
            model_type: entry.model_type,
            description: entry.description,
            base_price: entry.base_price,
            formula: entry.formula,
            api_schema: entry.api_schema,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_adapter_preserves_provider_schema_and_registry_preimage() {
        let provider = serde_json::json!({"model_id":"owner/model","name":"model","type":"text-to-image","description":"open","base_price":0.25,"formula":null,"api_schema":{"api_schemas":[{"type":"model_run","request_schema":{"properties":{"provider_input":{"type":"string"}}}}],"provider_extension":true}});
        let entry: ModelEntry = serde_json::from_value(provider.clone()).unwrap();
        assert_eq!(serde_json::to_value(&entry).unwrap(), provider);
        let wire = serde_json::to_value(ModelResourceOutput::from(entry)).unwrap();
        assert_eq!(wire["apiSchema"], provider["api_schema"]);
        assert_eq!(wire["type"], provider["type"]);
        let schema = serde_json::to_value(schemars::schema_for!(ModelResourceOutput)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(&wire));
        for (current, retired) in [
            ("modelId", "model_id"),
            ("basePrice", "base_price"),
            ("apiSchema", "api_schema"),
        ] {
            for mode in ["replacement", "mixed", "conflicting"] {
                let mut bad = wire.clone();
                let object = bad.as_object_mut().unwrap();
                let value = object.get(current).cloned().unwrap();
                if mode == "replacement" {
                    object.remove(current);
                }
                object.insert(
                    retired.into(),
                    if mode == "conflicting" {
                        serde_json::json!("retired-conflict")
                    } else {
                        value
                    },
                );
                assert!(!validator.is_valid(&bad));
                assert!(serde_json::from_value::<ModelResourceOutput>(bad).is_err());
            }
        }
    }
}
