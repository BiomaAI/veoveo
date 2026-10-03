//! Resource payload handling, separate from transport and Task orchestration.
use super::{
    BATCH_ARTIFACT_MIME, FramesMcp,
    outputs::usage_record,
    ownership::{frame_scope_from_identity, operation_scope_from_identity, runtime_owner},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use veoveo_frames_mcp::contract::{
    FRAME_USAGE_PAGE_SIZE, FrameUsageCursor, FrameUsageIndexUri, FrameUsagePage, FrameWorldsUri,
    FramesResource,
};
use veoveo_mcp_contract::{
    UsageReport,
    hosting::{gateway_identity, json_read, plane_caller, served_by_host},
};
use veoveo_task_runtime::TaskUsageAccess;

impl FramesMcp {
    /// Reads one admitted address. The host serves documents and the contract.
    pub(super) async fn read_frames_resource(
        &self,
        resource: FramesResource,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = gateway_identity(context)?;
        match resource {
            FramesResource::Docs | FramesResource::Document(_) | FramesResource::Contract => {
                Err(served_by_host())
            }
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
                json_read(uri, &worlds)
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
                json_read(uri, &page)
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
                json_read(uri, &frame)
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
                json_read(uri, &revision)
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
                json_read(uri, &world)
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
                json_read(uri, &operation)
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
                json_read(uri, &report)
            }
            FramesResource::Artifact(artifact_id) => {
                let caller = plane_caller(context)?;
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
}
