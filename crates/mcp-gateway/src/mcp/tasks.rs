use std::collections::BTreeSet;

use rmcp::{
    model::{
        CancelTaskParams, CancelTaskRequest, CancelTaskResult, ClientRequest,
        ErrorData as McpError, GetTaskParams, GetTaskPayloadParams, GetTaskPayloadRequest,
        GetTaskPayloadResult, GetTaskRequest, GetTaskResult, ListTasksResult,
        PaginatedRequestParams, ServerResult,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{
    GatewayAction, PolicyTarget, ServerSlug, TaskExposure as ContractTaskExposure, paginate,
};

use crate::mcp_support::{
    mcp_internal, mcp_invalid_params, mcp_invalid_request, project_task_payload_result,
    unexpected_upstream_response, upstream_error,
};

use super::{GATEWAY_PAGE_SIZE, GatewayMcp};

impl GatewayMcp {
    pub(super) fn profile_task_servers(&self) -> Vec<ServerSlug> {
        self.catalog
            .current()
            .profile_servers(&self.profile_id)
            .into_iter()
            .filter(|(exposure, server)| {
                exposure.tasks == ContractTaskExposure::Enabled && server.capabilities.tasks
            })
            .map(|(_, server)| server.slug.clone())
            .collect()
    }

    pub(super) async fn handle_list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let subject = self.authenticated(&context)?;
        let task_servers = self
            .profile_task_servers()
            .into_iter()
            .collect::<BTreeSet<_>>();
        if task_servers.is_empty() {
            return Err(mcp_invalid_request("profile does not expose MCP tasks"));
        }
        let mut allowed_task_servers = BTreeSet::new();
        for server in task_servers {
            let allowed = self.allows(
                &context,
                GatewayAction::TasksList,
                PolicyTarget::TaskList {
                    server: server.clone(),
                },
            )?;
            if allowed {
                allowed_task_servers.insert(server);
            }
        }

        let all_mappings = self
            .state
            .task_mappings_for_profile_owner(&self.profile_id, &subject.principal.id)
            .map_err(|err| mcp_internal(format!("failed to read gateway task mappings: {err}")))?;
        let mut mappings = Vec::new();
        for mapping in all_mappings {
            if !allowed_task_servers.contains(&mapping.upstream_server) {
                continue;
            }
            mappings.push(mapping);
        }

        let page = paginate(mappings, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        let mut tasks = Vec::with_capacity(page.items.len());
        for mapping in page.items {
            let server = mapping.upstream_server.clone();
            let upstream = self
                .upstream(&server, context.peer.clone(), &subject)
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
                    tasks.push(result.task);
                }
                other => return Err(unexpected_upstream_response("tasks/get", other)),
            }
        }

        let mut result = ListTasksResult::new(tasks);
        result.next_cursor = page.next_cursor;
        Ok(result)
    }

    pub(super) async fn handle_get_task_info(
        &self,
        mut request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let mapping = self.task_mapping(&request.task_id)?;
        let server = mapping.upstream_server.clone();
        let subject = self.authorize_mapped_task(&context, GatewayAction::TasksGet, &mapping)?;
        request.task_id = mapping.upstream_task_id.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::GetTaskRequest(GetTaskRequest::new(request)))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::GetTaskResult(mut result) => {
                result.task.task_id = mapping.gateway_task_id.to_string();
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tasks/get", other)),
        }
    }

    pub(super) async fn handle_get_task_result(
        &self,
        mut request: GetTaskPayloadParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        let mapping = self.task_mapping(&request.task_id)?;
        let server = mapping.upstream_server.clone();
        let subject = self.authorize_mapped_task(&context, GatewayAction::TasksResult, &mapping)?;
        request.task_id = mapping.upstream_task_id.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::GetTaskPayloadRequest(
                GetTaskPayloadRequest::new(request),
            ))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::GetTaskPayloadResult(mut result) => {
                project_task_payload_result(&mut result, &mapping)?;
                Ok(result)
            }
            ServerResult::CallToolResult(result) => {
                let payload = serde_json::to_value(result)
                    .map_err(|err| mcp_internal(format!("failed to encode task payload: {err}")))?;
                let mut result = GetTaskPayloadResult::new(payload);
                project_task_payload_result(&mut result, &mapping)?;
                Ok(result)
            }
            ServerResult::CustomResult(result) => {
                let mut result = GetTaskPayloadResult::new(result.0);
                project_task_payload_result(&mut result, &mapping)?;
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tasks/result", other)),
        }
    }

    pub(super) async fn handle_cancel_task(
        &self,
        mut request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        let mapping = self.task_mapping(&request.task_id)?;
        let server = mapping.upstream_server.clone();
        let subject = self.authorize_mapped_task(&context, GatewayAction::TasksCancel, &mapping)?;
        request.task_id = mapping.upstream_task_id.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::CancelTaskRequest(CancelTaskRequest::new(
                request,
            )))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::CancelTaskResult(mut result) => {
                result.task.task_id = mapping.gateway_task_id.to_string();
                Ok(result)
            }
            ServerResult::GetTaskResult(upstream_result) => {
                let mut task = upstream_result.task;
                task.task_id = mapping.gateway_task_id.to_string();
                let mut result = CancelTaskResult::new(task);
                result.meta = upstream_result.meta;
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tasks/cancel", other)),
        }
    }
}
