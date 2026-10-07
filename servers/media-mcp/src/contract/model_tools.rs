use super::{MediaModelId, MediaModelUri};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
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
    #[schemars(range(min = 1, max = 100))]
    pub limit: Option<u32>,
    #[serde(default)]
    pub cursor: Option<super::MediaModelCursor>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelSchemaArgs {
    /// Exact model id, e.g. wavespeed-ai/flux-schnell.
    pub model: MediaModelId,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "ModelCatalogItem")]
pub struct ModelCatalogItemValue {
    pub model_id: MediaModelId,
    pub name: String,
    #[serde(rename = "type")]
    pub model_type: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_price: Option<f64>,
    pub schema_uri: MediaModelUri,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "ModelCatalogOutput", transform = require_cursor_presence)]
pub struct ModelCatalogOutputValue {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub model_type: Option<String>,
    pub total_available: usize,
    #[schemars(range(max = 100))]
    pub returned: usize,
    #[schemars(length(max = 100))]
    pub models: Vec<ModelCatalogItem>,
    #[schemars(range(min = 1, max = 100))]
    pub limit: u32,
    #[serde(deserialize_with = "required_cursor")]
    #[schemars(required, schema_with = "nullable_cursor")]
    pub next_cursor: Option<super::MediaModelCursor>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(rename = "ModelSchemaOutput")]
pub struct ModelSchemaOutputValue {
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

impl TryFrom<super::ModelEntry> for ModelSchemaOutput {
    type Error = super::ModelCatalogError;
    fn try_from(model: super::ModelEntry) -> Result<Self, Self::Error> {
        let request_schema = model.request_schema().cloned();
        ModelSchemaOutputValue {
            schema_uri: MediaModelUri::new(model.model_id.clone()),
            model_id: model.model_id,
            name: model.name,
            model_type: model.model_type,
            description: model.description,
            base_price: model.base_price,
            formula: model.formula,
            request_schema,
        }
        .build()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ModelCatalogItemValue", into = "ModelCatalogItemValue")]
pub struct ModelCatalogItem(veoveo_types::Checked<ModelCatalogItemValue>);
impl std::ops::Deref for ModelCatalogItem {
    type Target = ModelCatalogItemValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ModelCatalogItem {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ModelCatalogItem".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ModelCatalogItemValue::json_schema(generator)
    }
}
impl TryFrom<ModelCatalogItemValue> for ModelCatalogItem {
    type Error = super::ModelCatalogError;
    fn try_from(value: ModelCatalogItemValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ModelCatalogItem> for ModelCatalogItemValue {
    fn from(value: ModelCatalogItem) -> Self {
        value.0.into_inner()
    }
}
impl ModelCatalogItemValue {
    pub fn build(self) -> Result<ModelCatalogItem, super::ModelCatalogError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ModelCatalogItemValue {
    type Error = super::ModelCatalogError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.schema_uri.model_id() != &self.model_id
            || self.base_price.is_some_and(|p| !p.is_finite())
        {
            return Err(super::ModelCatalogError);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ModelSchemaOutputValue", into = "ModelSchemaOutputValue")]
pub struct ModelSchemaOutput(veoveo_types::Checked<ModelSchemaOutputValue>);
impl std::ops::Deref for ModelSchemaOutput {
    type Target = ModelSchemaOutputValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ModelSchemaOutput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ModelSchemaOutput".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ModelSchemaOutputValue::json_schema(generator)
    }
}
impl TryFrom<ModelSchemaOutputValue> for ModelSchemaOutput {
    type Error = super::ModelCatalogError;
    fn try_from(value: ModelSchemaOutputValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ModelSchemaOutput> for ModelSchemaOutputValue {
    fn from(value: ModelSchemaOutput) -> Self {
        value.0.into_inner()
    }
}
impl ModelSchemaOutputValue {
    pub fn build(self) -> Result<ModelSchemaOutput, super::ModelCatalogError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ModelSchemaOutputValue {
    type Error = super::ModelCatalogError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.schema_uri.model_id() != &self.model_id
            || self.base_price.is_some_and(|p| !p.is_finite())
        {
            return Err(super::ModelCatalogError);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "ModelCatalogOutputValue", into = "ModelCatalogOutputValue")]
pub struct ModelCatalogOutput(veoveo_types::Checked<ModelCatalogOutputValue>);
impl std::ops::Deref for ModelCatalogOutput {
    type Target = ModelCatalogOutputValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for ModelCatalogOutput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ModelCatalogOutput".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ModelCatalogOutputValue::json_schema(generator)
    }
}
impl TryFrom<ModelCatalogOutputValue> for ModelCatalogOutput {
    type Error = super::ModelCatalogError;
    fn try_from(value: ModelCatalogOutputValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<ModelCatalogOutput> for ModelCatalogOutputValue {
    fn from(value: ModelCatalogOutput) -> Self {
        value.0.into_inner()
    }
}
impl ModelCatalogOutputValue {
    pub fn build(self) -> Result<ModelCatalogOutput, super::ModelCatalogError> {
        self.try_into()
    }
}
impl veoveo_types::Check for ModelCatalogOutputValue {
    type Error = super::ModelCatalogError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.returned != self.models.len()
            || self.returned > self.total_available
            || !(1..=100).contains(&self.limit)
            || self.returned > self.limit as usize
            || self
                .models
                .windows(2)
                .any(|w| w[0].model_id >= w[1].model_id)
            || self.query != super::normalized_model_filter(self.query.clone())
            || self.model_type != super::normalized_model_filter(self.model_type.clone())
        {
            return Err(super::ModelCatalogError);
        }
        if let Some(cursor) = &self.next_cursor {
            cursor.check_page(self)?;
        }
        Ok(())
    }
}

fn required_cursor<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<super::MediaModelCursor>, D::Error> {
    Option::deserialize(deserializer)
}
fn nullable_cursor(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    <Option<super::MediaModelCursor>>::json_schema(generator)
}

pub(super) fn require_cursor_presence(schema: &mut schemars::Schema) {
    let required = schema
        .as_object_mut()
        .expect("model page schema is an object")
        .entry("required")
        .or_insert_with(|| serde_json::json!([]));
    let fields = required.as_array_mut().expect("required is an array");
    if !fields.iter().any(|field| field == "nextCursor") {
        fields.push(serde_json::json!("nextCursor"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_catalog_root_requires_nullable_cursor_and_rejects_unknown_fields() {
        let page = super::super::model_catalog_page(
            &[],
            ModelsArgs {
                query: None,
                model_type: None,
                limit: Some(100),
                cursor: None,
            },
        )
        .unwrap();
        let wire = serde_json::to_value(&page).unwrap();
        let schema = serde_json::to_value(schemars::schema_for!(ModelCatalogOutput)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(&wire));
        assert_eq!(wire["nextCursor"], serde_json::Value::Null);
        assert_eq!(
            serde_json::from_value::<ModelCatalogOutput>(wire.clone()).unwrap(),
            page
        );
        for control in 0..3 {
            let mut bad = wire.clone();
            match control {
                0 => {
                    bad.as_object_mut().unwrap().remove("nextCursor");
                }
                1 => bad["limit"] = 101.into(),
                _ => bad["unknown"] = true.into(),
            }
            assert!(!validator.is_valid(&bad), "control {control}");
            assert!(
                serde_json::from_value::<ModelCatalogOutput>(bad).is_err(),
                "control {control}"
            );
        }
    }
}
