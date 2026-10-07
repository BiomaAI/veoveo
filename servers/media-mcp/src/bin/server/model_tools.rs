use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, ContentBlock},
};
use veoveo_media_mcp::contract::{ModelEntry, ModelSchemaOutput, ModelsArgs};

pub(super) fn models_result(
    models: &[ModelEntry],
    args: ModelsArgs,
) -> Result<CallToolResult, McpError> {
    let output = veoveo_media_mcp::contract::model_catalog_page(models, args)
        .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
    call_result(
        format!(
            "Found {} matching media model(s), returning {}. Use exact `modelId` values with media__run.",
            output.total_available, output.returned
        ),
        output,
    )
}

pub(super) fn model_schema_result(model: ModelEntry) -> Result<CallToolResult, McpError> {
    let output = ModelSchemaOutput::try_from(model)
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_media_mcp::contract::{ModelCatalogOutput, ModelSchemaOutput};

    fn model(model_id: &str, model_type: &str, description: &str) -> ModelEntry {
        ModelEntry {
            model_id: model_id.parse().unwrap(),
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
                cursor: None,
            },
        )
        .unwrap();
        let output: ModelCatalogOutput =
            serde_json::from_value(result.structured_content.unwrap()).unwrap();
        assert_eq!(output.total_available, 1);
        assert_eq!(
            output.models[0].model_id.as_str(),
            "wavespeed-ai/flux-schnell"
        );
    }

    #[test]
    fn model_schema_result_returns_request_schema() {
        let result = model_schema_result(model(
            "wavespeed-ai/flux-schnell",
            "text-to-image",
            "fast image generation",
        ))
        .unwrap();
        let wire = result.structured_content.as_ref().unwrap();
        let schema = serde_json::to_value(schemars::schema_for!(ModelSchemaOutput)).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(wire));
        for (current, retired) in [
            ("modelId", "model_id"),
            ("basePrice", "base_price"),
            ("schemaUri", "schema_uri"),
            ("requestSchema", "request_schema"),
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
                assert!(serde_json::from_value::<ModelSchemaOutput>(bad).is_err());
            }
        }
        let output: ModelSchemaOutput =
            serde_json::from_value(result.structured_content.unwrap()).unwrap();
        assert_eq!(output.model_id.as_str(), "wavespeed-ai/flux-schnell");
        assert!(
            output
                .request_schema
                .as_ref()
                .and_then(|schema| schema.get("required"))
                .is_some()
        );
    }
}

#[cfg(test)]
mod paging_tests {
    use super::*;
    use veoveo_media_mcp::contract::{
        MediaModelIndexUri, MediaResource, ModelCatalogOutput, model_catalog_page,
    };
    #[test]
    fn complete_catalog_traverses_tool_and_resource_pages_and_rejects_refresh() {
        let models: Vec<_> = (0..237)
            .map(|n| ModelEntry {
                model_id: format!("test/model-{n:03}").parse().unwrap(),
                name: format!("Model {n}"),
                model_type: "image".into(),
                description: "Open provider description".into(),
                base_price: Some(-0.1),
                formula: None,
                api_schema: Some(serde_json::json!({"extension": {"provider": true}})),
            })
            .collect();
        let mut cursor = None;
        let mut seen = std::collections::BTreeSet::new();
        loop {
            let args = ModelsArgs {
                query: Some(" MODEL ".into()),
                model_type: Some(" IMAGE ".into()),
                limit: Some(100),
                cursor: cursor.clone(),
            };
            let result = models_result(&models, args.clone()).unwrap();
            let tool: ModelCatalogOutput =
                serde_json::from_value(result.structured_content.unwrap()).unwrap();
            let uri = MediaModelIndexUri::new(
                args.query.as_ref(),
                args.model_type.as_ref(),
                args.limit.as_ref(),
                args.cursor.as_ref(),
            );
            let resource = MediaResource::parse(uri.as_str()).unwrap();
            let MediaResource::Models(uri) = resource else {
                panic!("model page")
            };
            let page = model_catalog_page(&models, uri.arguments()).unwrap();
            assert_eq!(
                serde_json::to_value(&tool).unwrap(),
                serde_json::to_value(&page).unwrap()
            );
            for item in &page.models {
                assert!(seen.insert(item.model_id.clone()));
            }
            cursor = page.next_cursor.clone();
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(seen.len(), 237);
        let first = model_catalog_page(
            &models,
            ModelsArgs {
                query: None,
                model_type: None,
                limit: None,
                cursor: None,
            },
        )
        .unwrap();
        let mut args = ModelsArgs {
            query: None,
            model_type: None,
            limit: None,
            cursor: first.next_cursor.clone(),
        };
        let mut refreshed = models.clone();
        refreshed[0].description.push_str(" refreshed");
        assert!(model_catalog_page(&refreshed, args.clone()).is_err());
        args.query = Some("changed".into());
        assert!(model_catalog_page(&models, args).is_err());
        let mut duplicate = models.clone();
        duplicate.push(models[0].clone());
        assert!(
            model_catalog_page(
                &duplicate,
                ModelsArgs {
                    query: None,
                    model_type: None,
                    limit: None,
                    cursor: None
                }
            )
            .is_err()
        );
        let current = serde_json::to_value(&first).unwrap();
        for (key, old, nested) in [
            ("nextCursor", "next_cursor", false),
            ("totalAvailable", "total_available", false),
            ("modelId", "model_id", true),
            ("schemaUri", "schema_uri", true),
        ] {
            for mixed in [false, true] {
                let mut bad = current.clone();
                let object = if nested {
                    bad["models"][0].as_object_mut().unwrap()
                } else {
                    bad.as_object_mut().unwrap()
                };
                let value = object[key].clone();
                if !mixed {
                    object.remove(key);
                }
                object.insert(old.into(), value);
                assert!(
                    serde_json::from_value::<ModelCatalogOutput>(bad).is_err(),
                    "{old} mixed={mixed}"
                );
            }
        }
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(first.next_cursor.as_ref().unwrap().as_str())
            .unwrap();
        let envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(envelope["version"], 2);
        assert!(envelope.get("modelType").is_some());
        for mutation in 0..3 {
            let mut bad = envelope.clone();
            if mutation == 0 {
                bad["version"] = 1.into();
            } else {
                let value = bad["modelType"].clone();
                if mutation == 1 {
                    bad.as_object_mut().unwrap().remove("modelType");
                }
                bad["model_type"] = value;
            }
            let wire = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(serde_json::to_vec(&bad).unwrap());
            assert!(veoveo_media_mcp::contract::MediaModelCursor::parse(wire).is_err());
        }
        let mut value = serde_json::to_value(&first).unwrap();
        value["returned"] = serde_json::json!(0);
        assert!(serde_json::from_value::<ModelCatalogOutput>(value).is_err());
        let mut value = serde_json::to_value(&first).unwrap();
        value["models"][0]["schemaUri"] = serde_json::json!("media://model/foreign/model");
        assert!(serde_json::from_value::<ModelCatalogOutput>(value).is_err());
        let mut value = serde_json::to_value(&first).unwrap();
        value.as_object_mut().unwrap().remove("nextCursor");
        assert!(serde_json::from_value::<ModelCatalogOutput>(value).is_err());
        assert!(veoveo_media_mcp::contract::MediaModelCursor::parse("not-a-cursor").is_err());
        let empty = model_catalog_page(
            &[],
            ModelsArgs {
                query: None,
                model_type: None,
                limit: None,
                cursor: None,
            },
        )
        .unwrap();
        assert_eq!(empty.returned, 0);
        assert!(empty.next_cursor.is_none());
    }
}
