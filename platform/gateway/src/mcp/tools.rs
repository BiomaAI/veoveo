use std::borrow::Cow;

use futures::StreamExt;
use rmcp::{
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResult, ClientRequest, CreateTaskResult,
        ErrorData as McpError, ListToolsResult, Notification, PaginatedRequestParams, ServerResult,
        Task, TaskStatus, TaskStatusNotificationParam, TaskSupport, Tool,
    },
    service::{PeerRequestOptions, RequestContext, RoleServer},
};
use serde_json::Value;
use veoveo_mcp_contract::{GatewayAction, LocalToolName, paginate, related_task_meta};
use veoveo_mcp_task_extension::{
    CLIENT_CAPABILITIES_META_KEY, DetailedTask, PROTOCOL_VERSION_META_KEY, ProtocolTaskId,
    RequestMeta, TASK_RETENTION_PIN_META_KEY, Task as FinalTask, TaskStatus as FinalTaskStatus,
    ToolCallParams,
};
use veoveo_task_runtime::TaskRetentionPin;

use crate::mcp_support::{
    mcp_internal, mcp_invalid_params, parse_gateway_tool, project_call_tool_resource_uris,
    project_tool_resource_metadata, unexpected_upstream_response, upstream_error,
};

use super::{GATEWAY_PAGE_SIZE, GatewayMcp};

impl GatewayMcp {
    pub(super) async fn handle_list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let subject = self.authenticated(&context)?;
        let project_tasks = self.client_allows_task_projection(&subject)?;
        let mut tools = Vec::new();
        for server_slug in self.profile_servers() {
            let catalog = self.catalog.current();
            let manifest = catalog
                .server(&server_slug)
                .ok_or_else(|| mcp_internal(format!("unknown profile server `{server_slug}`")))?;
            let final_tasks = if project_tasks && manifest.capabilities.tasks {
                let client = self.final_task_client(&server_slug, &subject).await?;
                client.discover().await.is_ok()
            } else {
                false
            };
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for mut tool in upstream.list_all_tools().await.map_err(upstream_error)? {
                let local_tool =
                    LocalToolName::new(tool.name.as_ref().to_owned()).map_err(|err| {
                        mcp_internal(format!("upstream exposed invalid tool name: {err}"))
                    })?;
                if !self.client_allows_compatibility_helper(&subject, &server_slug, &local_tool)? {
                    continue;
                }
                if !self
                    .allows_tool(
                        &context,
                        GatewayAction::ToolsList,
                        server_slug.clone(),
                        local_tool.clone(),
                    )
                    .await?
                {
                    continue;
                }
                project_tool_resource_metadata(manifest, &mut tool)?;
                let gateway_name = catalog
                    .project_tool_name(&server_slug, &local_tool)
                    .map_err(|err| mcp_internal(format!("failed to project tool name: {err}")))?;
                tool.name = Cow::Owned(gateway_name.to_string());
                if final_tasks {
                    adapt_task_tool(&mut tool);
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
            )
            .await?;
            return Err(mcp_invalid_params("unknown tool"));
        }
        let subject = self
            .authorize_tool(
                &context,
                GatewayAction::ToolsCall,
                projection.server.clone(),
                projection.tool.clone(),
            )
            .await?;
        request.name = Cow::Owned(projection.tool.to_string());
        let downstream_progress_token = context.meta.get_progress_token();
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
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

    pub(super) async fn handle_enqueue_task(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        let catalog = self.catalog.current();
        let projection = parse_gateway_tool(&catalog, &request.name)?;
        let subject = self.authenticated(&context)?;
        if !self.client_allows_task_projection(&subject)? {
            return Err(mcp_invalid_params("unknown method"));
        }
        let subject = self
            .authorize_tool(
                &context,
                GatewayAction::ToolsCall,
                projection.server.clone(),
                projection.tool.clone(),
            )
            .await?;
        let final_request = final_tool_request(request, projection.tool.as_str())?;
        let client = self.final_task_client(&projection.server, &subject).await?;
        let created = client.start_tool(final_request).await?;
        let task_id = created.task.task_id;
        let task = project_task_from_seed(created.task);
        self.forward_task_updates(
            client,
            task_id,
            context.peer.clone(),
            subject.principal.id.clone(),
            projection.server,
        )
        .await?;
        Ok(CreateTaskResult::new(task).with_meta(related_task_meta(task_id.to_string())))
    }

    async fn forward_task_updates(
        &self,
        client: super::final_tasks::FinalTaskClient,
        task_id: ProtocolTaskId,
        downstream: rmcp::service::Peer<RoleServer>,
        principal: veoveo_mcp_contract::PrincipalId,
        server: veoveo_mcp_contract::ServerSlug,
    ) -> Result<(), McpError> {
        let mut updates = client.subscribe(vec![task_id]).await?;
        let profile = self.profile_id.clone();
        tokio::spawn(async move {
            while let Some(update) = updates.next().await {
                let update = match update {
                    Ok(update) => update,
                    Err(error) => {
                        tracing::warn!(%profile, %principal, %server, %task_id, %error, "task projection subscription ended");
                        break;
                    }
                };
                let task = project_task_from_detailed(&update);
                let notification = rmcp::model::ServerNotification::TaskStatusNotification(
                    Notification::new(TaskStatusNotificationParam::new(task)),
                );
                if let Err(error) = downstream.send_notification(notification).await {
                    tracing::warn!(%profile, %principal, %server, %task_id, %error, "failed to forward task projection notification");
                    break;
                }
                if update.status().is_terminal() {
                    break;
                }
            }
        });
        Ok(())
    }
}

fn final_tool_request(
    request: CallToolRequestParams,
    upstream_name: &str,
) -> Result<ToolCallParams, McpError> {
    let mut meta = RequestMeta::new().with_task_capability();
    if let Some(request_meta) = request.meta {
        for (key, value) in request_meta.0 {
            if key == TASK_RETENTION_PIN_META_KEY {
                let pin: TaskRetentionPin = serde_json::from_value(value).map_err(|error| {
                    mcp_invalid_params(format!("invalid task retention pin: {error}"))
                })?;
                meta = meta.with_retention_pin(pin);
            } else if key == PROTOCOL_VERSION_META_KEY || key == CLIENT_CAPABILITIES_META_KEY {
                continue;
            } else {
                meta.additional.insert(key, value);
            }
        }
    }
    Ok(ToolCallParams {
        meta,
        name: upstream_name.to_owned(),
        arguments: request.arguments.unwrap_or_default().into_iter().collect(),
    })
}

fn adapt_task_tool(tool: &mut Tool) {
    tool.execution = Some(
        tool.execution
            .take()
            .unwrap_or_default()
            .with_task_support(TaskSupport::Optional),
    );
}

pub(super) fn project_task_from_seed(task: FinalTask) -> Task {
    let mut projected = Task::new(
        task.task_id.to_string(),
        project_task_status(task.status),
        task.created_at.to_rfc3339(),
        task.last_updated_at.to_rfc3339(),
    );
    projected.status_message = task.status_message;
    projected.ttl = task.ttl_ms;
    projected.poll_interval = task.poll_interval_ms;
    projected
}

pub(super) fn project_task_from_detailed(task: &DetailedTask) -> Task {
    let metadata = task.metadata();
    let mut projected = Task::new(
        metadata.task_id.to_string(),
        project_task_status(task.status()),
        metadata.created_at.to_rfc3339(),
        metadata.last_updated_at.to_rfc3339(),
    );
    projected.status_message = metadata.status_message.clone();
    projected.ttl = metadata.ttl_ms;
    projected.poll_interval = metadata.poll_interval_ms;
    projected
}

fn project_task_status(status: FinalTaskStatus) -> TaskStatus {
    match status {
        FinalTaskStatus::Working => TaskStatus::Working,
        FinalTaskStatus::InputRequired => TaskStatus::InputRequired,
        FinalTaskStatus::Completed => TaskStatus::Completed,
        FinalTaskStatus::Cancelled => TaskStatus::Cancelled,
        FinalTaskStatus::Failed => TaskStatus::Failed,
    }
}

pub(super) fn completed_tool_result(task: DetailedTask) -> Result<CallToolResult, McpError> {
    match task {
        DetailedTask::Completed { result, .. } => {
            serde_json::from_value(Value::Object(result.into_iter().collect())).map_err(|error| {
                mcp_internal(format!(
                    "upstream task result was not a tool result: {error}"
                ))
            })
        }
        DetailedTask::Failed { error, .. } => Err(McpError::new(
            rmcp::model::ErrorCode(error.code),
            error.message,
            error.data,
        )),
        DetailedTask::Cancelled { .. } => {
            Err(McpError::invalid_request("task was cancelled", None))
        }
        DetailedTask::Working { .. } | DetailedTask::InputRequired { .. } => {
            Err(mcp_internal("task result requested before completion"))
        }
    }
}

trait FinalTaskStatusExt {
    fn is_terminal(&self) -> bool;
}

impl FinalTaskStatusExt for FinalTaskStatus {
    fn is_terminal(&self) -> bool {
        matches!(
            *self,
            FinalTaskStatus::Completed | FinalTaskStatus::Cancelled | FinalTaskStatus::Failed
        )
    }
}

#[cfg(test)]
mod tests {
    use rmcp::model::{JsonObject, ToolExecution};

    use super::*;

    #[test]
    fn final_task_tool_is_optional_only_at_the_rig_projection() {
        let mut tool = Tool::new("forecast", "Forecast.", JsonObject::new())
            .with_execution(ToolExecution::new());
        adapt_task_tool(&mut tool);
        assert_eq!(tool.task_support(), TaskSupport::Optional);
    }

    #[test]
    fn retention_pin_is_preserved_in_final_request_meta() {
        let mut request = CallToolRequestParams::new("timeseries__forecast");
        let mut meta = rmcp::model::Meta::new();
        meta.0.insert(
            TASK_RETENTION_PIN_META_KEY.to_owned(),
            serde_json::json!("agent-episode:test"),
        );
        request.meta = Some(meta);
        let projected = final_tool_request(request, "forecast").unwrap();
        assert_eq!(
            projected
                .meta
                .task_retention_pin
                .as_ref()
                .map(TaskRetentionPin::as_str),
            Some("agent-episode:test")
        );
    }
}
