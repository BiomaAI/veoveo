//! Resource catalogs and payloads; domain selection belongs to the library.
use super::{
    MediaMcp, SERVER_DOCS,
    ownership::{internal_caller, internal_identity, runtime_owner},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents, ResourceTemplate,
    },
    service::RequestContext,
};
use veoveo_mcp_contract::UsageReport;
use veoveo_media_mcp::{
    contract::{
        MediaGenerationUri, MediaPredictionIndexUri, MediaPredictionUri, MediaTaskUsageUri,
        MediaUsageIndexUri,
    },
    reads::MediaReads,
    uris,
};

fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![
        Resource::new(uris::DOCS_URI, "docs")
            .with_title("Server documents")
            .with_description("Index of the crate documents embedded at build time.")
            .with_mime_type("application/json"),
    ];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(uris::doc_uri(doc.id), doc.title)
                .with_title(doc.title)
                .with_description("Crate document embedded at build time.")
                .with_mime_type("text/markdown"),
        );
    }
    resources.push(
        Resource::new(uris::CONTRACT_URI, "contract")
            .with_title("Contract declaration")
            .with_description(
                "Machine-readable contract revision, compliance, and capability inventory.",
            )
            .with_mime_type("application/json"),
    );
    resources
}

/// Templates served by `list_resource_templates` and declared in the
/// `media://contract` capability inventory.
pub(super) fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(uris::DOC_TEMPLATE, "doc")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        ResourceTemplate::new(uris::MODEL_TEMPLATE, "model")
            .with_title("Media model schema")
            .with_description(
                "Full definition of one model: input JSON Schema, pricing, description. \
                     model_id supports completion/complete.",
            )
            .with_mime_type("application/json"),
        ResourceTemplate::new(MediaPredictionUri::TEMPLATE, "prediction")
            .with_title("Media prediction state")
            .with_description(
                "Live state of a prediction. Subscribable: resources/updated fires when \
                     the provider reports a terminal state.",
            )
            .with_mime_type("application/json"),
        ResourceTemplate::new(MediaGenerationUri::TEMPLATE, "generation_result")
            .with_title("Media generation result")
            .with_description(
                "Completed generation and its published output metadata, retained with the Task.",
            )
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
            .with_title("Media artifact")
            .with_description(
                "Server-owned immutable output artifact, addressed by occurrence id.",
            ),
        ResourceTemplate::new(MediaUsageIndexUri::TEMPLATE, "usage_page")
            .with_title("Media usage page")
            .with_mime_type("application/json"),
        ResourceTemplate::new(MediaPredictionIndexUri::TEMPLATE, "prediction_page")
            .with_title("Media prediction page")
            .with_mime_type("application/json"),
        ResourceTemplate::new(MediaTaskUsageUri::TEMPLATE, "usage")
            .with_title("Media task usage")
            .with_description("Usage estimates and actuals for one task, addressed by task id.")
            .with_mime_type("application/json"),
    ]
}

pub(super) fn resource_catalog() -> Vec<Resource> {
    let mut resources = well_known_resources();
    resources.extend([
        veoveo_mcp_apps_extension::app_resource(uris::STUDIO_APP_URI, "studio")
            .with_title("Studio")
            .with_description(
                "Generate media through governed provider models and inspect outputs.",
            ),
        Resource::new(uris::MODELS_URI, "models")
            .with_title("Media model catalog")
            .with_description(
                "Compact index of every media model: model_id, type, description, base price.",
            )
            .with_mime_type("application/json"),
        Resource::new(MediaPredictionIndexUri::ROOT, "predictions")
            .with_title("Media predictions")
            .with_description("Paged prediction identities visible to the caller.")
            .with_mime_type("application/json"),
        Resource::new(MediaUsageIndexUri::ROOT, "usage")
            .with_title("Media usage ledger")
            .with_description("Index of task usage resources.")
            .with_mime_type("application/json"),
    ]);
    resources.sort_by(|a, b| a.uri.cmp(&b.uri));
    resources
}

impl MediaMcp {
    pub(super) async fn read_media_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        async {
            let identity = internal_identity(&context)?;
            let uri = request.uri.as_str();
            // Well-known surface (contract C18, C19): readable by any
            // authenticated identity, like `list_resources`.
            if uri == uris::DOCS_URI {
                let text = serde_json::to_string(&SERVER_DOCS.iter().collect::<Vec<_>>())
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(text, uri).with_mime_type("application/json"),
                ]));
            }
            if let Some(doc_id) = uris::parse_doc_uri(uri) {
                let doc = SERVER_DOCS.doc(doc_id).ok_or_else(|| {
                    McpError::resource_not_found(
                        format!("unknown server document '{doc_id}'"),
                        None,
                    )
                })?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                ]));
            }
            if uri == uris::CONTRACT_URI {
                let declaration = SERVER_DOCS.contract_declaration();
                let text = serde_json::to_string(declaration)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(text, uri).with_mime_type("application/json"),
                ]));
            }
            if uri == uris::STUDIO_APP_URI {
                let html = veoveo_mcp_apps_extension::workbench_app_html(
                    &veoveo_mcp_apps_extension::WorkbenchApp {
                        app_id: "media-studio",
                        title: "Studio",
                        subtitle: "Choose a model, run generation, and check usage",
                        empty_message: "No media models are available.",
                        resources: &[
                            veoveo_mcp_apps_extension::WorkbenchResource {
                                label: "Model catalog",
                                uri: uris::MODELS_URI,
                            },
                            veoveo_mcp_apps_extension::WorkbenchResource {
                                label: "Predictions",
                                uri: MediaPredictionIndexUri::ROOT,
                            },
                            veoveo_mcp_apps_extension::WorkbenchResource {
                                label: "Usage ledger",
                                uri: MediaUsageIndexUri::ROOT,
                            },
                        ],
                        tools: &[
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Search models",
                                name: "models",
                                arguments_json: r#"{"query":"","limit":20}"#,
                            },
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Inspect model schema",
                                name: "model_schema",
                                arguments_json: r#"{"model":""}"#,
                            },
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Generate media",
                                name: "run",
                                arguments_json: r#"{"model":"","input":{}}"#,
                            },
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Inspect artifact",
                                name: "artifact",
                                arguments_json: r#"{"artifact_uri":"media://artifact/"}"#,
                            },
                        ],
                        stream_result: None,
                    },
                );
                return Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(uri, &html),
                ]));
            }
            let text = if uri == uris::MODELS_URI {
                let models = self
                    .state
                    .registry()
                    .await
                    .map_err(|e| McpError::internal_error(e, None))?;
                Self::models_index_json(&models).to_string()
            } else if let Ok(index) = MediaUsageIndexUri::parse(uri) {
                encode(
                    &MediaReads::new(&self.state.tasks)
                        .map_err(internal)?
                        .usage_page(&runtime_owner(&identity), index.cursor())
                        .await
                        .map_err(internal)?,
                )?
            } else if let Ok(index) = MediaPredictionIndexUri::parse(uri) {
                encode(
                    &MediaReads::new(&self.state.tasks)
                        .map_err(internal)?
                        .predictions(&runtime_owner(&identity), index.cursor())
                        .await
                        .map_err(internal)?,
                )?
            } else if let Some(model_id) = uris::parse_model_uri(uri) {
                let entry = self
                    .state
                    .find_model(model_id)
                    .await
                    .map_err(|e| McpError::internal_error(e, None))?
                    .ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("unknown model '{model_id}'; browse media://models"),
                            None,
                        )
                    })?;
                serde_json::to_string(&entry)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?
            } else if let Ok(address) = MediaGenerationUri::parse(uri) {
                let result = MediaReads::new(&self.state.tasks)
                    .map_err(internal)?
                    .generation_result(&runtime_owner(&identity), &address)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| {
                        McpError::resource_not_found("unknown generation result", None)
                    })?;
                encode(&result)?
            } else if let Ok(address) = MediaPredictionUri::parse(uri) {
                let summary = MediaReads::new(&self.state.tasks)
                    .map_err(internal)?
                    .prediction(&runtime_owner(&identity), &address)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| McpError::resource_not_found("unknown prediction", None))?;
                encode(&summary)?
            } else if let Ok(address) = MediaTaskUsageUri::parse(uri) {
                let rows = MediaReads::new(&self.state.tasks)
                    .map_err(internal)?
                    .usage(&runtime_owner(&identity), &address)
                    .await
                    .map_err(internal)?;
                if rows.is_empty() {
                    return Err(McpError::resource_not_found("unknown task usage", None));
                }
                encode(
                    &UsageReport::new(address.task_id().to_string(), address.as_str())
                        .with_records(rows),
                )?
            } else if let Some(artifact_id) = uris::parse_artifact_uri(uri) {
                // The plane enforces access with the caller's identity.
                let caller = internal_caller(&context)?;
                let artifact = self
                    .state
                    .artifacts
                    .get(&caller, &artifact_id)
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?
                    .ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("unknown artifact '{artifact_id}'"),
                            None,
                        )
                    })?;
                let blob = BASE64_STANDARD.encode(&artifact.bytes);
                let mut content = ResourceContents::blob(blob, uri);
                if let Some(mime) = artifact.metadata.mime_type {
                    content = content.with_mime_type(mime);
                }
                return Ok(ReadResourceResult::new(vec![content]));
            } else {
                return Err(McpError::resource_not_found(
                    format!("unknown resource uri: {uri}"),
                    None,
                ));
            };
            Ok(ReadResourceResult::new(vec![
                ResourceContents::text(text, uri).with_mime_type("application/json"),
            ]))
        }
        .await
        .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}
fn encode(value: &impl serde::Serialize) -> Result<String, McpError> {
    serde_json::to_string(value).map_err(internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_declares_static_roots_and_typed_page_templates() {
        let resources = resource_catalog();
        let uris = resources
            .iter()
            .map(|resource| resource.uri.as_str())
            .collect::<Vec<_>>();
        assert!(uris.contains(&MediaUsageIndexUri::ROOT));
        assert!(uris.contains(&MediaPredictionIndexUri::ROOT));
        assert!(uris.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            !uris.iter().any(|uri| uri.starts_with("media://usage/task/")
                || uri.starts_with("media://prediction/"))
        );
        let templates = resource_templates();
        for expected in [
            MediaUsageIndexUri::TEMPLATE,
            MediaPredictionIndexUri::TEMPLATE,
            MediaTaskUsageUri::TEMPLATE,
            MediaPredictionUri::TEMPLATE,
            MediaGenerationUri::TEMPLATE,
        ] {
            assert!(
                templates
                    .iter()
                    .any(|template| template.uri_template == expected)
            );
        }
    }
}
