use std::borrow::Cow;

use rmcp::{
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResult, ClientRequest, CreateTaskResult,
        ErrorData as McpError, ListToolsResult, PaginatedRequestParams, ServerResult,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{
    GatewayAction, GatewayTaskId, GatewayTaskMapping, LocalToolName, UpstreamTaskId, paginate,
};

use crate::mcp_support::{
    mcp_internal, mcp_invalid_params, parse_gateway_tool, unexpected_upstream_response,
    upstream_error,
};

use super::{GATEWAY_PAGE_SIZE, GatewayMcp};

impl GatewayMcp {
    pub(super) async fn handle_list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let subject = self.authenticated(&context)?;
        let mut tools = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for mut tool in upstream.list_all_tools().await.map_err(upstream_error)? {
                let local_tool =
                    LocalToolName::new(tool.name.as_ref().to_string()).map_err(|err| {
                        mcp_internal(format!("upstream exposed invalid tool name: {err}"))
                    })?;
                if !self.allows_tool(
                    &context,
                    GatewayAction::ToolsList,
                    server_slug.clone(),
                    local_tool.clone(),
                )? {
                    continue;
                }
                let gateway_name = self
                    .catalog
                    .current()
                    .project_tool_name(&server_slug, &local_tool)
                    .map_err(|err| mcp_internal(format!("failed to project tool name: {err}")))?;
                tool.name = Cow::Owned(gateway_name.to_string());
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
        let subject = self.authorize_tool(
            &context,
            GatewayAction::ToolsCall,
            projection.server.clone(),
            projection.tool.clone(),
        )?;
        request.name = Cow::Owned(projection.tool.to_string());
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        upstream.call_tool(request).await.map_err(upstream_error)
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
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::CallToolRequest(CallToolRequest::new(
                request,
            )))
            .await
            .map_err(upstream_error)?;
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
                        upstream_server: projection.server.clone(),
                        upstream_task_id,
                        profile: self.profile_id.clone(),
                        owner: subject.principal.id.clone(),
                        created_at: now,
                        updated_at: now,
                    })
                    .map_err(|err| {
                        mcp_internal(format!("failed to persist gateway task mapping: {err}"))
                    })?;
                result.task.task_id = gateway_task_id.to_string();
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tools/call task", other)),
        }
    }
}
