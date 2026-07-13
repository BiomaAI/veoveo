use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, ContentBlock},
    schemars,
};
use serde_json::Value;
use veoveo_media_mcp::{provider::ModelEntry, uris};

const DEFAULT_MODEL_LIMIT: usize = 20;
const MAX_MODEL_LIMIT: usize = 100;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub(super) struct ModelsArgs {
    /// Case-insensitive search over model id, name, type, and description.
    #[serde(default)]
    pub(super) query: Option<String>,
    /// Exact model type filter, e.g. text-to-image, image-to-image, text-to-video.
    #[serde(default, rename = "type")]
    pub(super) model_type: Option<String>,
    /// Maximum number of models to return. Defaults to 20 and cannot exceed 100.
    #[serde(default)]
    pub(super) limit: Option<u32>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub(super) struct ModelSchemaArgs {
    /// Exact model id, e.g. wavespeed-ai/flux-schnell.
    pub(super) model: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(super) struct ModelCatalogItem {
    pub(super) model_id: String,
    pub(super) name: String,
    #[serde(rename = "type")]
    pub(super) model_type: String,
    pub(super) description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) base_price: Option<f64>,
    pub(super) schema_uri: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(super) struct ModelCatalogOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "type")]
    pub(super) model_type: Option<String>,
    pub(super) total_available: usize,
    pub(super) returned: usize,
    pub(super) models: Vec<ModelCatalogItem>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(super) struct ModelSchemaOutput {
    pub(super) model_id: String,
    pub(super) name: String,
    #[serde(rename = "type")]
    pub(super) model_type: String,
    pub(super) description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) base_price: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) formula: Option<String>,
    pub(super) schema_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) request_schema: Option<Value>,
}

pub(super) fn models_result(
    models: &[ModelEntry],
    args: ModelsArgs,
) -> Result<CallToolResult, McpError> {
    let limit = validated_limit(args.limit)?;
    let query = normalized_filter(args.query);
    let model_type = normalized_filter(args.model_type);
    let mut filtered: Vec<ModelCatalogItem> = models
        .iter()
        .filter(|model| matches_type(model, model_type.as_deref()))
        .filter(|model| matches_query(model, query.as_deref()))
        .map(catalog_item)
        .collect();
    filtered.sort_by(|left, right| left.model_id.cmp(&right.model_id));
    let total_available = filtered.len();
    filtered.truncate(limit);
    let output = ModelCatalogOutput {
        query,
        model_type,
        total_available,
        returned: filtered.len(),
        models: filtered,
    };
    call_result(
        format!(
            "Found {} matching media model(s), returning {}. Use exact `model_id` values with media__run.",
            output.total_available, output.returned
        ),
        output,
    )
}

pub(super) fn model_schema_result(model: ModelEntry) -> Result<CallToolResult, McpError> {
    let request_schema = model.request_schema().cloned();
    let output = ModelSchemaOutput {
        model_id: model.model_id.clone(),
        name: model.name,
        model_type: model.model_type,
        description: model.description,
        base_price: model.base_price,
        formula: model.formula,
        schema_uri: uris::model_uri(&model.model_id),
        request_schema,
    };
    call_result(
        format!(
            "Schema for {}. Pass this exact model id as `model` to media__run.",
            output.model_id
        ),
        output,
    )
}

fn call_result<T: serde::Serialize>(text: String, output: T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(output).map_err(|err| {
        McpError::internal_error(format!("failed to encode model tool output: {err}"), None)
    })?);
    Ok(result)
}

fn validated_limit(limit: Option<u32>) -> Result<usize, McpError> {
    let limit = limit
        .map(usize::try_from)
        .transpose()
        .map_err(|_| McpError::invalid_params("limit does not fit this platform", None))?
        .unwrap_or(DEFAULT_MODEL_LIMIT);
    if limit == 0 || limit > MAX_MODEL_LIMIT {
        return Err(McpError::invalid_params(
            format!("limit must be between 1 and {MAX_MODEL_LIMIT}"),
            None,
        ));
    }
    Ok(limit)
}

fn normalized_filter(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

fn matches_type(model: &ModelEntry, model_type: Option<&str>) -> bool {
    model_type
        .map(|expected| model.model_type.eq_ignore_ascii_case(expected))
        .unwrap_or(true)
}

fn matches_query(model: &ModelEntry, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return true;
    };
    model.model_id.to_ascii_lowercase().contains(query)
        || model.name.to_ascii_lowercase().contains(query)
        || model.model_type.to_ascii_lowercase().contains(query)
        || model.description.to_ascii_lowercase().contains(query)
}

fn catalog_item(model: &ModelEntry) -> ModelCatalogItem {
    ModelCatalogItem {
        model_id: model.model_id.clone(),
        name: model.name.clone(),
        model_type: model.model_type.clone(),
        description: model.description.clone(),
        base_price: model.base_price,
        schema_uri: uris::model_uri(&model.model_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(model_id: &str, model_type: &str, description: &str) -> ModelEntry {
        ModelEntry {
            model_id: model_id.to_string(),
            name: model_id.to_string(),
            model_type: model_type.to_string(),
            description: description.to_string(),
            base_price: Some(0.003),
            formula: None,
            api_schema: Some(serde_json::json!({
                "api_schemas": [
                    {
                        "type": "model_run",
                        "request_schema": {
                            "type": "object",
                            "required": ["prompt"],
                            "properties": {
                                "prompt": {"type": "string"}
                            }
                        }
                    }
                ]
            })),
        }
    }

    #[test]
    fn models_result_filters_by_query_and_type() {
        let result = models_result(
            &[
                model(
                    "wavespeed-ai/flux-schnell",
                    "text-to-image",
                    "fast image generation",
                ),
                model(
                    "openai/gpt-image-2/text-to-image",
                    "text-to-image",
                    "OpenAI image generation",
                ),
                model(
                    "luma/ray-3.2/text-to-video",
                    "text-to-video",
                    "video generation",
                ),
            ],
            ModelsArgs {
                query: Some("flux".to_string()),
                model_type: Some("text-to-image".to_string()),
                limit: Some(10),
            },
        )
        .unwrap();
        let output: ModelCatalogOutput =
            serde_json::from_value(result.structured_content.unwrap()).unwrap();
        assert_eq!(output.total_available, 1);
        assert_eq!(output.models[0].model_id, "wavespeed-ai/flux-schnell");
    }

    #[test]
    fn model_schema_result_returns_request_schema() {
        let result = model_schema_result(model(
            "wavespeed-ai/flux-schnell",
            "text-to-image",
            "fast image generation",
        ))
        .unwrap();
        let output: ModelSchemaOutput =
            serde_json::from_value(result.structured_content.unwrap()).unwrap();
        assert_eq!(output.model_id, "wavespeed-ai/flux-schnell");
        assert!(
            output
                .request_schema
                .as_ref()
                .and_then(|schema| schema.get("required"))
                .is_some()
        );
    }
}
