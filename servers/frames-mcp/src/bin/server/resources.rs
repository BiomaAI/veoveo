//! Resource payload handling, separate from transport and Task orchestration.
use super::{
    BATCH_ARTIFACT_MIME, FramesMcp, SERVER_DOCS, SERVER_SLUG,
    outputs::usage_record,
    ownership::{
        frame_scope_from_identity, internal_caller, internal_identity, optional_task_owner,
        require_task_owner, task_owner_allows,
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ReadResourceRequestParams, ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use serde::Serialize;
use serde_json::json;
use veoveo_frames_mcp::{
    contract::{CoordinateOperationId, FrameWorldsUri},
    uris,
};
use veoveo_mcp_contract::UsageReport;

impl FramesMcp {
    pub(super) async fn read_frames_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        async {
            let uri = request.uri.as_str();
            let identity = internal_identity(&context)?;
            // Well-known surface (contract C18, C19): readable by any
            // authenticated identity, like `list_resources`.
            if uri == uris::DOCS_URI {
                return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
            }
            if let Some(doc_id) = uris::parse_doc_uri(uri) {
                let doc = SERVER_DOCS.doc(doc_id).ok_or_else(|| {
                    McpError::resource_not_found(
                        format!("unknown server document `{doc_id}`"),
                        None,
                    )
                })?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                ]));
            }
            if uri == uris::CONTRACT_URI {
                return json_resource(uri, SERVER_DOCS.contract_declaration());
            }
            if uri == uris::WORKSPACE_APP_URI {
                let html = veoveo_mcp_apps_extension::workbench_app_html(
                    &veoveo_mcp_apps_extension::WorkbenchApp {
                        app_id: "frames-workspace",
                        title: "Frame Editor",
                        subtitle: "Build frame worlds and convert coordinates between frames",
                        empty_message: "No frame worlds are visible to this identity.",
                        resources: &[
                            veoveo_mcp_apps_extension::WorkbenchResource {
                                label: "Frame worlds",
                                uri: FrameWorldsUri::ROOT,
                            },
                            veoveo_mcp_apps_extension::WorkbenchResource {
                                label: "Usage",
                                uri: uris::USAGE_ROOT_URI,
                            },
                        ],
                        tools: &[
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Convert frame",
                                name: "convert_frame",
                                arguments_json: "{}",
                            },
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Create world",
                                name: "create_world",
                                arguments_json: "{}",
                            },
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Publish world",
                                name: "publish_world",
                                arguments_json: "{}",
                            },
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Batch transform",
                                name: "batch_transform",
                                arguments_json: "{}",
                            },
                        ],
                        stream_result: None,
                    },
                );
                return Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(uri, &html),
                ]));
            }
            let scope = frame_scope_from_identity(&self.state, &identity).await?;
            if let Ok(catalog) = FrameWorldsUri::parse(uri) {
                let worlds = self
                    .state
                    .frames
                    .worlds_page(&scope, catalog.cursor())
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?;
                return json_resource(uri, &worlds);
            }
            if uri == uris::USAGE_ROOT_URI {
                let mut entries = Vec::new();
                for task_id in self
                    .state
                    .tasks
                    .platform_store()
                    .domain_usage_task_ids(SERVER_SLUG)
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?
                {
                    let task_id = task_id.to_string();
                    let Some(owner) = optional_task_owner(&self.state, &task_id).await? else {
                        continue;
                    };
                    if task_owner_allows(&owner, &identity) {
                        entries.push(json!({
                            "task_id": task_id,
                            "usage_uri": uris::usage_task_uri(&task_id),
                        }));
                    }
                }
                return json_resource(uri, &entries);
            }
            if let Some(frame_uri) = uris::parse_world_frame_uri(uri) {
                let frame = self
                    .state
                    .frames
                    .get_frame(&scope, &frame_uri)
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?
                    .ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("unknown world frame `{frame_uri}`"),
                            None,
                        )
                    })?;
                return json_resource(uri, &frame);
            }
            if let Some(revision_uri) = uris::parse_world_revision_uri(uri) {
                let revision = self
                    .state
                    .frames
                    .get_revision(&scope, &revision_uri)
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?
                    .ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("unknown frame world revision `{revision_uri}`"),
                            None,
                        )
                    })?;
                return json_resource(uri, &revision);
            }
            if let Some(world_uri) = uris::parse_world_uri(uri) {
                let world_id = world_uri.world_id();
                let world = self
                    .state
                    .frames
                    .get_world(&scope, &world_id)
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?
                    .ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("unknown frame world `{world_id}`"),
                            None,
                        )
                    })?;
                return json_resource(uri, &world);
            }
            if let Some(operation_id) = uris::parse_operation_uri(uri) {
                let operation_id = CoordinateOperationId::new(operation_id)
                    .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
                let operation = self
                    .state
                    .frames
                    .get_operation(&scope, &operation_id)
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?
                    .ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("unknown operation `{operation_id}`"),
                            None,
                        )
                    })?;
                return json_resource(uri, &operation);
            }
            if let Some(task_id) = uris::parse_usage_task_uri(uri) {
                require_task_owner(&self.state, &context, task_id).await?;
                let task_uuid = task_id
                    .parse::<veoveo_types::TaskId>()
                    .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
                let records = self
                    .state
                    .tasks
                    .platform_store()
                    .domain_usage_for_task(SERVER_SLUG, task_uuid)
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?;
                let report: UsageReport = UsageReport::new(task_id, uris::usage_task_uri(task_id))
                    .with_records(
                        records
                            .into_iter()
                            .map(|record| usage_record(task_id, record))
                            .collect(),
                    );
                if report.records.is_empty() {
                    return Err(McpError::resource_not_found(
                        format!("unknown usage task `{task_id}`"),
                        None,
                    ));
                }
                return json_resource(uri, &report);
            }
            if let Some(artifact_id) = uris::parse_artifact_uri(uri) {
                let caller = internal_caller(&context)?;
                let artifact = self
                    .state
                    .artifacts
                    .get(&caller, &artifact_id)
                    .await
                    .map_err(|err| McpError::internal_error(err.to_string(), None))?
                    .ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("unknown artifact `{artifact_id}`"),
                            None,
                        )
                    })?;
                let blob = BASE64_STANDARD.encode(&artifact.bytes);
                let mut content = ResourceContents::blob(blob, uri);
                content = content.with_mime_type(
                    artifact
                        .metadata
                        .mime_type
                        .unwrap_or_else(|| BATCH_ARTIFACT_MIME.to_string()),
                );
                return Ok(ReadResourceResult::new(vec![content]));
            }
            Err(McpError::resource_not_found(
                format!("unknown resource uri: {uri}"),
                None,
            ))
        }
        .await
        .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }
}

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, McpError> {
    let text = serde_json::to_string(value)
        .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(text, uri).with_mime_type("application/json"),
    ]))
}
