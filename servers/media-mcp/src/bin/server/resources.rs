//! Resource catalogs and payloads; domain selection belongs to the library.
use super::{MediaMcp, ownership::runtime_owner};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use veoveo_mcp_contract::{
    UsageReport,
    hosting::{gateway_identity, json_read, plane_caller, served_by_host},
};
use veoveo_media_mcp::{
    contract::{MediaPredictionIndexUri, MediaResource, MediaUsageIndexUri},
    reads::MediaReads,
};

impl MediaMcp {
    /// Reads one admitted address. The host serves documents and the contract.
    pub(super) async fn read_media_resource(
        &self,
        resource: MediaResource,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = gateway_identity(context)?;
        let owner = runtime_owner(&identity);
        match resource {
            MediaResource::Docs | MediaResource::Document(_) | MediaResource::Contract => {
                Err(served_by_host())
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
                                arguments_json: r#"{"artifactUri":"media://artifact/"}"#,
                            },
                        ],
                        stream_result: None,
                    },
                );
                Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(uri, &html),
                ]))
            }
            MediaResource::Models(page) => {
                let models = self
                    .state
                    .registry()
                    .await
                    .map_err(|e| McpError::internal_error(e, None))?;
                json_read(
                    uri,
                    &veoveo_media_mcp::contract::model_catalog_page(&models, page.arguments())
                        .map_err(|error| McpError::invalid_params(error.to_string(), None))?,
                )
            }
            MediaResource::Usage(index) => json_read(
                uri,
                &self
                    .reads()?
                    .usage_page(&owner, index.cursor())
                    .await
                    .map_err(internal)?,
            ),
            MediaResource::Predictions(index) => json_read(
                uri,
                &self
                    .reads()?
                    .predictions(&owner, index.cursor())
                    .await
                    .map_err(internal)?,
            ),
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
                json_read(
                    uri,
                    &veoveo_media_mcp::contract::ModelResourceOutput::from(entry),
                )
            }
            MediaResource::Generation(address) => {
                let result = self
                    .reads()?
                    .generation_result(&owner, &address)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| {
                        McpError::resource_not_found("unknown generation result", None)
                    })?;
                json_read(uri, &result)
            }
            MediaResource::Prediction(address) => {
                let summary = self
                    .reads()?
                    .prediction(&owner, &address)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| McpError::resource_not_found("unknown prediction", None))?;
                json_read(uri, &summary)
            }
            MediaResource::TaskUsage(address) => {
                let rows = self
                    .reads()?
                    .usage(&owner, &address)
                    .await
                    .map_err(internal)?;
                if rows.is_empty() {
                    return Err(McpError::resource_not_found("unknown task usage", None));
                }
                json_read(
                    uri,
                    &UsageReport::new(address.task_id().to_string(), address.as_str())
                        .with_records(rows),
                )
            }
            MediaResource::Artifact(address) => {
                let artifact_id = address.artifact_id();
                // The plane enforces access with the caller's identity.
                let caller = plane_caller(context)?;
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
                Ok(ReadResourceResult::new(vec![content]))
            }
        }
    }

    fn reads(&self) -> Result<MediaReads<'_>, McpError> {
        MediaReads::new(&self.state.tasks).map_err(internal)
    }
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}
