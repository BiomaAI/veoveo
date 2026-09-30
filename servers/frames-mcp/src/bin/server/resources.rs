//! Resource payload handling, separate from transport and Task orchestration.
use super::{
    BATCH_ARTIFACT_MIME, FramesMcp, SERVER_DOCS,
    outputs::usage_record,
    ownership::{
        frame_scope_from_identity, internal_caller, internal_identity,
        operation_scope_from_identity, runtime_owner,
    },
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ReadResourceRequestParams, ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use serde::Serialize;
use veoveo_frames_mcp::contract::{
    FRAME_USAGE_PAGE_SIZE, FrameUsageCursor, FrameUsageIndexUri, FrameUsagePage, FrameWorldsUri,
    FramesResource,
};
use veoveo_mcp_contract::UsageReport;
use veoveo_task_runtime::TaskUsageAccess;

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
            let resource = FramesResource::parse(uri)
                .map_err(|_| McpError::resource_not_found("unknown resource", None))?;
            match resource {
                // Well-known surface (contract C18, C19): readable by any
                // authenticated identity, like `list_resources`.
                FramesResource::Docs => json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>()),
                FramesResource::Document(doc_id) => {
                    let doc_id = doc_id.as_str();
                    let doc = SERVER_DOCS.doc(doc_id).ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("unknown server document `{doc_id}`"),
                            None,
                        )
                    })?;
                    Ok(ReadResourceResult::new(vec![
                        ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                    ]))
                }
                FramesResource::Contract => json_resource(uri, SERVER_DOCS.contract_declaration()),
                FramesResource::WorkspaceApp => {
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
                                    uri: FrameUsageIndexUri::ROOT,
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
                    Ok(ReadResourceResult::new(vec![
                        veoveo_mcp_apps_extension::app_html_contents(uri, &html),
                    ]))
                }
                FramesResource::Worlds(catalog) => {
                    let scope = frame_scope_from_identity(&self.state, &identity).await?;
                    let worlds = self
                        .state
                        .frames
                        .worlds_page(&scope, catalog.cursor())
                        .await
                        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
                    json_resource(uri, &worlds)
                }
                FramesResource::Usage(catalog) => {
                    let page = self
                        .state
                        .tasks
                        .usage_page(
                            TaskUsageAccess::Owner(&runtime_owner(&identity)),
                            catalog.cursor().map(FrameUsageCursor::after),
                            FRAME_USAGE_PAGE_SIZE,
                        )
                        .await
                        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
                    let page = FrameUsagePage::from_task_ids(page.task_ids, page.next_task_id)
                        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
                    json_resource(uri, &page)
                }
                FramesResource::Frame(frame_uri) => {
                    let scope = frame_scope_from_identity(&self.state, &identity).await?;
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
                    json_resource(uri, &frame)
                }
                FramesResource::Revision(revision_uri) => {
                    let scope = frame_scope_from_identity(&self.state, &identity).await?;
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
                    json_resource(uri, &revision)
                }
                FramesResource::World(world_uri) => {
                    let scope = frame_scope_from_identity(&self.state, &identity).await?;
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
                    json_resource(uri, &world)
                }
                FramesResource::Operation(operation_uri) => {
                    let operation_scope = operation_scope_from_identity(&identity);
                    let operation = self
                        .state
                        .frames
                        .get_operation(&operation_scope, &operation_uri)
                        .await
                        .map_err(|error| McpError::internal_error(error.to_string(), None))?
                        .ok_or_else(|| {
                            McpError::resource_not_found(
                                format!("unknown operation `{operation_uri}`"),
                                None,
                            )
                        })?;
                    json_resource(uri, &operation)
                }
                FramesResource::TaskUsage(usage_uri) => {
                    let task_id = usage_uri.task_id();
                    let records = self
                        .state
                        .tasks
                        .usage(TaskUsageAccess::Owner(&runtime_owner(&identity)), task_id)
                        .await
                        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
                    let report: UsageReport =
                        UsageReport::new(task_id.to_string(), usage_uri.to_string()).with_records(
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
                    json_resource(uri, &report)
                }
                FramesResource::Artifact(artifact_id) => {
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
                    Ok(ReadResourceResult::new(vec![content]))
                }
            }
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
