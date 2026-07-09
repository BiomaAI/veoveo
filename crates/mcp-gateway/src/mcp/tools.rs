use std::{borrow::Cow, time::Duration};

use rmcp::{
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResult, ClientRequest, ContentBlock,
        CreateTaskResult, ErrorData as McpError, GetTaskParams, GetTaskPayloadParams,
        GetTaskPayloadRequest, GetTaskRequest, ListToolsResult, PaginatedRequestParams,
        ReadResourceRequestParams, Resource, ResourceContents, ServerResult, Task, TaskStatus,
        TaskSupport, Tool, ToolAnnotations,
    },
    schemars,
    service::{Peer, PeerRequestOptions, RequestContext, RoleClient, RoleServer},
};
use serde_json::Value;
use tokio::time::{Instant, sleep};
use veoveo_mcp_contract::{
    GatewayAction, GatewayTaskId, GatewayTaskMapping, GatewayTaskStatus, GatewayTaskStatusDocument,
    LocalToolName, UpstreamTaskId, VEOVEO_TASK_RESULT_COMPATIBILITY_HELPER_ID,
    VEOVEO_TASK_RESULT_COMPATIBILITY_TOOL_NAME, paginate, parse_gateway_task_resource_uri,
    related_task_meta,
};

use crate::mcp_support::{
    mcp_internal, mcp_invalid_params, mcp_invalid_request, parse_gateway_tool,
    project_call_tool_resource_uris, project_call_tool_result, project_tool_resource_metadata,
    task_mapping_allows_principal, unexpected_upstream_response, upstream_error,
};

use super::{GATEWAY_PAGE_SIZE, GatewayMcp};

const DIRECT_TASK_WAIT_TIMEOUT: Duration = Duration::from_secs(20);
const DIRECT_TASK_DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const DIRECT_TASK_MIN_POLL_INTERVAL: Duration = Duration::from_millis(250);
const DIRECT_TASK_MAX_POLL_INTERVAL: Duration = Duration::from_secs(5);
const DIRECT_TASK_INLINE_IMAGE_MAX_BYTES: u64 = 3 * 1024 * 1024;
const DIRECT_TASK_DESCRIPTION: &str = "At this Veoveo profile, direct tool calls are supported: Veoveo creates the upstream MCP task, returns final output when ready quickly, or returns a Veoveo task id and veoveo://task/{task_id} status resource when still running.";

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TaskResultArgs {
    /// Canonical Veoveo task resource URI, e.g. veoveo://task/{task_id}.
    task_uri: String,
}

impl GatewayMcp {
    pub(super) async fn handle_list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let subject = self.authenticated(&context)?;
        let mut tools = Vec::new();
        if self.client_allows_gateway_compatibility_helper(
            &subject,
            &task_result_compatibility_helper_id()?,
        )? {
            tools.push(task_result_tool());
        }
        for server_slug in self.profile_servers() {
            let catalog = self.catalog.current();
            let manifest = catalog
                .server(&server_slug)
                .ok_or_else(|| mcp_internal(format!("unknown profile server `{server_slug}`")))?;
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for mut tool in upstream.list_all_tools().await.map_err(upstream_error)? {
                let local_tool =
                    LocalToolName::new(tool.name.as_ref().to_string()).map_err(|err| {
                        mcp_internal(format!("upstream exposed invalid tool name: {err}"))
                    })?;
                if !self.client_allows_compatibility_helper(&subject, &server_slug, &local_tool)? {
                    continue;
                }
                if !self.allows_tool(
                    &context,
                    GatewayAction::ToolsList,
                    server_slug.clone(),
                    local_tool.clone(),
                )? {
                    continue;
                }
                project_tool_resource_metadata(manifest, &mut tool)?;
                let gateway_name = catalog
                    .project_tool_name(&server_slug, &local_tool)
                    .map_err(|err| mcp_internal(format!("failed to project tool name: {err}")))?;
                tool.name = Cow::Owned(gateway_name.to_string());
                if self.client_allows_direct_task_adapter(&subject)? {
                    adapt_gateway_tool_execution(&mut tool);
                }
                tools.push(tool);
            }
        }
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let page = paginate(tools, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    pub(super) async fn handle_call_tool(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if request.name.as_ref() == VEOVEO_TASK_RESULT_COMPATIBILITY_TOOL_NAME {
            return self.handle_task_result_tool(request, context).await;
        }
        let catalog = self.catalog.current();
        let projection = parse_gateway_tool(&catalog, &request.name)?;
        let subject = self.authenticated(&context)?;
        if !self.client_allows_compatibility_helper(
            &subject,
            &projection.server,
            &projection.tool,
        )? {
            self.record_policy_denial(
                &subject,
                GatewayAction::ToolsCall,
                veoveo_mcp_contract::PolicyTarget::Tool {
                    server: projection.server.clone(),
                    tool: projection.tool.clone(),
                },
                veoveo_mcp_contract::PolicyReasonCode::UnknownTool,
            )?;
            return Err(mcp_invalid_params("unknown tool"));
        }
        let subject = self.authorize_tool(
            &context,
            GatewayAction::ToolsCall,
            projection.server.clone(),
            projection.tool.clone(),
        )?;
        request.name = Cow::Owned(projection.tool.to_string());
        let downstream_progress_token = context.meta.get_progress_token();
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        let task_support = upstream
            .list_all_tools()
            .await
            .map_err(upstream_error)?
            .into_iter()
            .find(|tool| tool.name.as_ref() == projection.tool.as_str())
            .ok_or_else(|| {
                mcp_internal(format!(
                    "upstream server `{}` did not expose expected tool `{}`",
                    projection.server, projection.tool
                ))
            })?
            .task_support();
        if task_support == TaskSupport::Required {
            if !self.client_allows_direct_task_adapter(&subject)? {
                return Err(mcp_invalid_request("tool requires task-based invocation"));
            }
            request.task = Some(request.task.unwrap_or_default());
            request.name = Cow::Owned(projection.tool.to_string());
            let created = self
                .enqueue_upstream_task(request, context.clone(), subject, projection.server)
                .await?;
            return self.await_direct_task_result(created.task, context).await;
        }
        let handle = upstream
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(request)),
                PeerRequestOptions::no_options(),
            )
            .await
            .map_err(upstream_error)?;
        if let Some(downstream_token) = downstream_progress_token {
            self.progress_tokens
                .register(
                    &self.profile_id,
                    &subject.principal.id,
                    &projection.server,
                    handle.progress_token.clone(),
                    downstream_token,
                )
                .await;
        }
        let upstream_token = handle.progress_token.clone();
        let result = handle.await_response().await.map_err(upstream_error);
        self.progress_tokens
            .remove_token(
                &self.profile_id,
                &subject.principal.id,
                &projection.server,
                &upstream_token,
            )
            .await;
        match result? {
            ServerResult::CallToolResult(mut result) => {
                let manifest = catalog.server(&projection.server).ok_or_else(|| {
                    mcp_internal(format!("unknown tool server `{}`", projection.server))
                })?;
                project_call_tool_resource_uris(manifest, &mut result)?;
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tools/call", other)),
        }
    }

    async fn handle_task_result_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let subject = self.authenticated(&context)?;
        if !self.client_allows_gateway_compatibility_helper(
            &subject,
            &task_result_compatibility_helper_id()?,
        )? {
            return Err(mcp_invalid_params("unknown tool"));
        }
        let args: TaskResultArgs = serde_json::from_value(Value::Object(
            request.arguments.unwrap_or_default(),
        ))
        .map_err(|err| mcp_invalid_params(format!("invalid task_result arguments: {err}")))?;
        let task_id = parse_gateway_task_resource_uri(&args.task_uri)
            .ok_or_else(|| mcp_invalid_params("task_uri must be veoveo://task/{task_id}"))?;
        let mapping = self.task_mapping(task_id)?;
        self.authorize_mapped_task(&context, GatewayAction::TasksGet, &mapping)?;
        let task = self.direct_task_status(task_id, &context).await?;
        match task.status {
            TaskStatus::Completed => {
                self.authorize_mapped_task(&context, GatewayAction::TasksResult, &mapping)?;
                self.direct_task_payload_result(task_id, &context).await
            }
            TaskStatus::Failed | TaskStatus::Cancelled => {
                direct_task_status_result(&task, None, true)
            }
            TaskStatus::Working | TaskStatus::InputRequired => {
                direct_task_status_result(&task, None, false)
            }
            _ => Err(mcp_internal(format!(
                "unsupported MCP task status: {:?}",
                task.status
            ))),
        }
    }

    pub(super) async fn handle_enqueue_task(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        let catalog = self.catalog.current();
        let projection = parse_gateway_tool(&catalog, &request.name)?;
        let subject = self.authorize_tool(
            &context,
            GatewayAction::ToolsCall,
            projection.server.clone(),
            projection.tool.clone(),
        )?;
        request.name = Cow::Owned(projection.tool.to_string());
        self.enqueue_upstream_task(request, context, subject, projection.server)
            .await
    }

    async fn enqueue_upstream_task(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
        subject: crate::AuthenticatedSubject,
        server: veoveo_mcp_contract::ServerSlug,
    ) -> Result<CreateTaskResult, McpError> {
        let downstream_progress_token = context.meta.get_progress_token();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let handle = upstream
            .send_cancellable_request(
                ClientRequest::CallToolRequest(CallToolRequest::new(request)),
                PeerRequestOptions::no_options(),
            )
            .await
            .map_err(upstream_error)?;
        if let Some(downstream_token) = downstream_progress_token {
            self.progress_tokens
                .register(
                    &self.profile_id,
                    &subject.principal.id,
                    &server,
                    handle.progress_token.clone(),
                    downstream_token,
                )
                .await;
        }
        let upstream_progress_token = handle.progress_token.clone();
        let result = match handle.await_response().await {
            Ok(result) => result,
            Err(err) => {
                self.progress_tokens
                    .remove_token(
                        &self.profile_id,
                        &subject.principal.id,
                        &server,
                        &upstream_progress_token,
                    )
                    .await;
                return Err(upstream_error(err));
            }
        };
        match result {
            ServerResult::CreateTaskResult(mut result) => {
                let upstream_task_id =
                    UpstreamTaskId::new(result.task.task_id.clone()).map_err(|err| {
                        mcp_internal(format!("upstream returned invalid task id: {err}"))
                    })?;
                let gateway_task_id = GatewayTaskId::new(uuid::Uuid::new_v4().to_string())
                    .map_err(|err| {
                        mcp_internal(format!("failed to create gateway task id: {err}"))
                    })?;
                let now = chrono::Utc::now();
                self.state
                    .record_task_mapping(&GatewayTaskMapping {
                        gateway_task_id: gateway_task_id.clone(),
                        upstream_server: server.clone(),
                        upstream_task_id: upstream_task_id.clone(),
                        profile: self.profile_id.clone(),
                        owner: subject.principal.id.clone(),
                        created_at: now,
                        updated_at: now,
                    })
                    .map_err(|err| {
                        mcp_internal(format!("failed to persist gateway task mapping: {err}"))
                    })?;
                self.progress_tokens
                    .attach_task(
                        &self.profile_id,
                        &subject.principal.id,
                        &server,
                        &upstream_progress_token,
                        upstream_task_id,
                    )
                    .await;
                result.task.task_id = gateway_task_id.to_string();
                result.meta = Some(related_task_meta(gateway_task_id.as_str()));
                Ok(result)
            }
            other => {
                self.progress_tokens
                    .remove_token(
                        &self.profile_id,
                        &subject.principal.id,
                        &server,
                        &upstream_progress_token,
                    )
                    .await;
                Err(unexpected_upstream_response("tools/call task", other))
            }
        }
    }

    async fn await_direct_task_result(
        &self,
        mut task: Task,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let deadline = Instant::now() + DIRECT_TASK_WAIT_TIMEOUT;
        loop {
            match task.status {
                TaskStatus::Completed => {
                    return self
                        .direct_task_payload_result(&task.task_id, &context)
                        .await;
                }
                TaskStatus::Failed | TaskStatus::Cancelled => {
                    return direct_task_status_result(&task, None, true);
                }
                TaskStatus::Working | TaskStatus::InputRequired => {}
                _ => {
                    return Err(mcp_internal(format!(
                        "unsupported MCP task status: {:?}",
                        task.status
                    )));
                }
            }

            let Some(sleep_for) = direct_task_sleep_duration(&task, deadline) else {
                return direct_task_status_result(&task, None, false);
            };
            sleep(sleep_for).await;
            task = self.direct_task_status(&task.task_id, &context).await?;
        }
    }

    pub(super) async fn direct_task_status(
        &self,
        task_id: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<Task, McpError> {
        let mapping = self.task_mapping(task_id)?;
        let subject = self.authenticated(context)?;
        if !task_mapping_allows_principal(&self.profile_id, &mapping, &subject.principal.id) {
            return Err(mcp_invalid_params("unknown gateway task id"));
        }
        let upstream = self
            .upstream(&mapping.upstream_server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::GetTaskRequest(GetTaskRequest::new(
                GetTaskParams::new(mapping.upstream_task_id.to_string()),
            )))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::GetTaskResult(mut result) => {
                result.task.task_id = mapping.gateway_task_id.to_string();
                Ok(result.task)
            }
            other => Err(unexpected_upstream_response("tasks/get", other)),
        }
    }

    pub(super) async fn direct_task_payload_result(
        &self,
        task_id: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let mapping = self.task_mapping(task_id)?;
        let subject = self.authenticated(context)?;
        if !task_mapping_allows_principal(&self.profile_id, &mapping, &subject.principal.id) {
            return Err(mcp_invalid_params("unknown gateway task id"));
        }
        let upstream = self
            .upstream(&mapping.upstream_server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::GetTaskPayloadRequest(
                GetTaskPayloadRequest::new(GetTaskPayloadParams::new(
                    mapping.upstream_task_id.to_string(),
                )),
            ))
            .await
            .map_err(upstream_error)?;
        let mut result = match result {
            ServerResult::GetTaskPayloadResult(payload) => {
                serde_json::from_value::<CallToolResult>(payload.0).map_err(|err| {
                    mcp_internal(format!(
                        "upstream task payload was not a tool result: {err}"
                    ))
                })?
            }
            ServerResult::CallToolResult(result) => result,
            ServerResult::CustomResult(result) => {
                serde_json::from_value::<CallToolResult>(result.0).map_err(|err| {
                    mcp_internal(format!(
                        "upstream custom task payload was not a tool result: {err}"
                    ))
                })?
            }
            other => return Err(unexpected_upstream_response("tasks/result", other)),
        };
        project_call_tool_result(&mut result, &mapping)?;
        if self.client_allows_direct_task_adapter(&subject)? {
            inline_generation_images(&mut result, &upstream).await?;
        }
        Ok(result)
    }
}

async fn inline_generation_images(
    result: &mut CallToolResult,
    upstream: &Peer<RoleClient>,
) -> Result<(), McpError> {
    let Some(structured) = &result.structured_content else {
        return Ok(());
    };
    let Ok(output) =
        serde_json::from_value::<veoveo_mcp_contract::GenerationRunOutput>(structured.clone())
    else {
        return Ok(());
    };
    if result
        .content
        .iter()
        .any(|block| block.as_image().is_some())
    {
        return Ok(());
    }
    for artifact in output.artifacts {
        let Some(mime_type) = artifact.mime_type.as_deref() else {
            continue;
        };
        if !mime_type.starts_with("image/")
            || artifact.byte_len > DIRECT_TASK_INLINE_IMAGE_MAX_BYTES
        {
            continue;
        }
        let resource = upstream
            .read_resource(ReadResourceRequestParams::new(
                artifact.artifact_uri.clone(),
            ))
            .await
            .map_err(upstream_error)?;
        for content in resource.contents {
            if let ResourceContents::BlobResourceContents {
                blob,
                mime_type: Some(mime),
                ..
            } = content
                && mime.starts_with("image/")
            {
                result.content.push(ContentBlock::image(blob, mime));
            }
        }
    }
    Ok(())
}

fn task_result_compatibility_helper_id()
-> Result<veoveo_mcp_contract::CompatibilityHelperId, McpError> {
    veoveo_mcp_contract::CompatibilityHelperId::new(
        VEOVEO_TASK_RESULT_COMPATIBILITY_HELPER_ID.to_string(),
    )
    .map_err(|err| mcp_internal(format!("invalid Veoveo compatibility helper id: {err}")))
}

fn task_result_tool() -> Tool {
    Tool::new(
        VEOVEO_TASK_RESULT_COMPATIBILITY_TOOL_NAME,
        "Read a Veoveo task status or final result by passing the veoveo://task/{task_id} URI returned from a previous tool call.",
        rmcp::handler::server::tool::schema_for_type::<TaskResultArgs>(),
    )
    .with_title("Get task result")
    .with_annotations(
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
}

fn adapt_gateway_tool_execution(tool: &mut Tool) {
    if tool.task_support() != TaskSupport::Required {
        return;
    }
    let execution = tool
        .execution
        .take()
        .unwrap_or_default()
        .with_task_support(TaskSupport::Optional);
    tool.execution = Some(execution);
    match &tool.description {
        Some(description) if description.contains(DIRECT_TASK_DESCRIPTION) => {}
        Some(description) => {
            tool.description = Some(Cow::Owned(format!(
                "{} Upstream contract: {}",
                DIRECT_TASK_DESCRIPTION, description
            )));
        }
        None => {
            tool.description = Some(Cow::Borrowed(DIRECT_TASK_DESCRIPTION));
        }
    }
}

fn direct_task_sleep_duration(task: &Task, deadline: Instant) -> Option<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return None;
    }
    let requested = task
        .poll_interval
        .map(Duration::from_millis)
        .unwrap_or(DIRECT_TASK_DEFAULT_POLL_INTERVAL)
        .max(DIRECT_TASK_MIN_POLL_INTERVAL)
        .min(DIRECT_TASK_MAX_POLL_INTERVAL);
    Some(requested.min(remaining))
}

fn direct_task_status_result(
    task: &Task,
    result: Option<Value>,
    is_error: bool,
) -> Result<CallToolResult, McpError> {
    let document = direct_task_status_document(task, result)?;
    let status_resource = document.task.status_resource.to_string();
    let mut blocks = vec![ContentBlock::text(direct_task_status_text(
        task,
        &status_resource,
    )?)];
    blocks.push(ContentBlock::resource_link(
        Resource::new(status_resource.clone(), format!("task {}", task.task_id))
            .with_title("Veoveo task status")
            .with_description("Veoveo task status and result document.")
            .with_mime_type("application/json"),
    ));
    let value = serde_json::to_value(document)
        .map_err(|err| mcp_internal(format!("failed to encode gateway task status: {err}")))?;
    let mut call_result = if is_error {
        CallToolResult::error(blocks)
    } else {
        CallToolResult::success(blocks)
    };
    call_result.structured_content = Some(value);
    call_result.meta = Some(related_task_meta(&task.task_id));
    Ok(call_result)
}

pub(super) fn direct_task_status_document(
    task: &Task,
    result: Option<Value>,
) -> Result<GatewayTaskStatusDocument, McpError> {
    Ok(GatewayTaskStatusDocument {
        task: GatewayTaskStatus::from_task(task)
            .map_err(|err| mcp_internal(format!("failed to build gateway task status: {err}")))?,
        result,
    })
}

fn direct_task_status_text(task: &Task, status_resource: &str) -> Result<String, McpError> {
    let status = format!("{:?}", task.status).to_lowercase();
    Ok(match task.status {
        TaskStatus::Failed | TaskStatus::Cancelled => {
            let message = task
                .status_message
                .as_deref()
                .unwrap_or("no status message");
            format!(
                "Veoveo task {} is {status}: {message}. Read {status_resource} for details.",
                task.task_id
            )
        }
        TaskStatus::Completed => {
            format!(
                "Veoveo task {} completed. Read {status_resource} for details.",
                task.task_id
            )
        }
        TaskStatus::Working | TaskStatus::InputRequired => {
            let poll = task.poll_interval.unwrap_or(
                u64::try_from(DIRECT_TASK_DEFAULT_POLL_INTERVAL.as_millis()).unwrap_or(1000),
            );
            format!(
                "Veoveo task {} is {status}. Read {status_resource} for status and result. Suggested check interval: {poll} ms.",
                task.task_id
            )
        }
        _ => {
            return Err(mcp_internal(format!(
                "unsupported MCP task status: {:?}",
                task.status
            )));
        }
    })
}

#[cfg(test)]
mod tests {
    use rmcp::model::{JsonObject, ToolExecution};
    use veoveo_mcp_contract::{GatewayTaskStatusKind, RELATED_TASK_META_KEY};

    use super::*;

    #[test]
    fn required_upstream_tool_is_optional_at_gateway_boundary() {
        let mut tool = Tool::new("run", "Run work.", JsonObject::new())
            .with_execution(ToolExecution::new().with_task_support(TaskSupport::Required));

        adapt_gateway_tool_execution(&mut tool);

        assert_eq!(tool.task_support(), TaskSupport::Optional);
        assert!(
            tool.description
                .as_ref()
                .is_some_and(|description| description.contains(DIRECT_TASK_DESCRIPTION))
        );
    }

    #[test]
    fn running_direct_task_result_contains_gateway_task_handle() {
        let now = chrono::Utc::now().to_rfc3339();
        let task = Task::new(
            "gateway-task-1".to_string(),
            TaskStatus::Working,
            now.clone(),
            now,
        )
        .with_status_message("accepted")
        .with_poll_interval(5000);

        let result = direct_task_status_result(&task, None, false).unwrap();

        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result
                .meta
                .as_ref()
                .and_then(|meta| meta.0.get(RELATED_TASK_META_KEY))
                .and_then(|value| value.get("taskId"))
                .and_then(Value::as_str),
            Some("gateway-task-1")
        );
        let document: GatewayTaskStatusDocument =
            serde_json::from_value(result.structured_content.unwrap()).unwrap();
        assert_eq!(document.task.task_id.as_str(), "gateway-task-1");
        assert_eq!(document.task.status, GatewayTaskStatusKind::Working);
        assert_eq!(
            document.task.status_resource.as_str(),
            "veoveo://task/gateway-task-1"
        );
        assert_eq!(document.task.poll_after_ms, Some(5000));
        assert!(document.result.is_none());
        assert!(result.content.iter().any(|block| {
            block
                .as_resource_link()
                .is_some_and(|resource| resource.uri == "veoveo://task/gateway-task-1")
        }));
    }
}
