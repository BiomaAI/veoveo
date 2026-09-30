//! Resource catalogs and payloads; domain selection belongs to the library.
use super::{
    MediaMcp, SERVER_DOCS,
    ownership::{internal_caller, internal_identity, runtime_owner},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ReadResourceRequestParams, ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use veoveo_mcp_contract::UsageReport;
use veoveo_media_mcp::{
    contract::{MediaPredictionIndexUri, MediaResource, MediaUsageIndexUri},
    reads::MediaReads,
};

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
            let resource = MediaResource::parse(uri)
                .map_err(|_| McpError::resource_not_found("unknown Media resource", None))?;
            let text = match resource {
                // Well-known surface (contract C18, C19): readable by any
                // authenticated identity, like `list_resources`.
                MediaResource::Docs => {
                    let text = serde_json::to_string(&SERVER_DOCS.iter().collect::<Vec<_>>())
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    return Ok(ReadResourceResult::new(vec![
                        ResourceContents::text(text, uri).with_mime_type("application/json"),
                    ]));
                }
                MediaResource::Document(doc_id) => {
                    let doc_id = doc_id.as_str();
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
                MediaResource::Contract => {
                    let declaration = SERVER_DOCS.contract_declaration();
                    let text = serde_json::to_string(declaration)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                    return Ok(ReadResourceResult::new(vec![
                        ResourceContents::text(text, uri).with_mime_type("application/json"),
                    ]));
                }
                MediaResource::StudioApp => {
                    let html = veoveo_mcp_apps_extension::workbench_app_html(
                        &veoveo_mcp_apps_extension::WorkbenchApp {
                            app_id: "media-studio",
                            title: "Studio",
                            subtitle: "Choose a model, run generation, and check usage",
                            empty_message: "No media models are available.",
                            resources: &[
                                veoveo_mcp_apps_extension::WorkbenchResource {
                                    label: "Model catalog",
                                    uri: veoveo_media_mcp::uris::MODELS_URI,
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
                MediaResource::Models => {
                    let models = self
                        .state
                        .registry()
                        .await
                        .map_err(|e| McpError::internal_error(e, None))?;
                    Self::models_index_json(&models).to_string()
                }
                MediaResource::Usage(index) => encode(
                    &MediaReads::new(&self.state.tasks)
                        .map_err(internal)?
                        .usage_page(&runtime_owner(&identity), index.cursor())
                        .await
                        .map_err(internal)?,
                )?,
                MediaResource::Predictions(index) => encode(
                    &MediaReads::new(&self.state.tasks)
                        .map_err(internal)?
                        .predictions(&runtime_owner(&identity), index.cursor())
                        .await
                        .map_err(internal)?,
                )?,
                MediaResource::Model(address) => {
                    let model_id = address.model_id();
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
                }
                MediaResource::Generation(address) => {
                    let result = MediaReads::new(&self.state.tasks)
                        .map_err(internal)?
                        .generation_result(&runtime_owner(&identity), &address)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| {
                            McpError::resource_not_found("unknown generation result", None)
                        })?;
                    encode(&result)?
                }
                MediaResource::Prediction(address) => {
                    let summary = MediaReads::new(&self.state.tasks)
                        .map_err(internal)?
                        .prediction(&runtime_owner(&identity), &address)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| McpError::resource_not_found("unknown prediction", None))?;
                    encode(&summary)?
                }
                MediaResource::TaskUsage(address) => {
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
                }
                MediaResource::Artifact(address) => {
                    let artifact_id = address.artifact_id();
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
                }
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
