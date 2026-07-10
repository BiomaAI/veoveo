use futures::StreamExt;
use rmcp::{
    model::{
        CancelTaskParams, CancelTaskResult, ErrorData as McpError, GetTaskParams,
        GetTaskPayloadParams, GetTaskPayloadResult, GetTaskResult,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::GatewayAction;
use veoveo_mcp_task_extension::DetailedTask;

use crate::mcp_support::{mcp_internal, mcp_invalid_params, project_call_tool_resource_uris};

use super::{
    GatewayMcp,
    tools::{completed_tool_result, project_task_from_detailed},
};

impl GatewayMcp {
    pub(super) async fn handle_get_task_info(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let subject = self.authenticated(&context)?;
        if !self.client_allows_direct_task_adapter(&subject)? {
            return Err(mcp_invalid_params("unknown method"));
        }
        let route = self
            .authorize_canonical_task(&context, GatewayAction::TasksGet, &request.task_id)
            .await?;
        let client = self
            .final_task_client(&route.server, &route.subject)
            .await?;
        let task = client.get(route.task_id).await?;
        Ok(GetTaskResult::new(project_task_from_detailed(&task)))
    }

    pub(super) async fn handle_get_task_result(
        &self,
        request: GetTaskPayloadParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        let subject = self.authenticated(&context)?;
        if !self.client_allows_direct_task_adapter(&subject)? {
            return Err(mcp_invalid_params("unknown method"));
        }
        let route = self
            .authorize_canonical_task(&context, GatewayAction::TasksResult, &request.task_id)
            .await?;
        let client = self
            .final_task_client(&route.server, &route.subject)
            .await?;
        let mut task = client.get(route.task_id).await?;
        if !is_terminal(&task) {
            let mut updates = client.subscribe(vec![route.task_id]).await?;
            while let Some(update) = updates.next().await {
                task = update?;
                if is_terminal(&task) {
                    break;
                }
            }
            if !is_terminal(&task) {
                return Err(mcp_internal(
                    "upstream task subscription ended before task completion",
                ));
            }
        }
        let mut result = completed_tool_result(task)?;
        let catalog = self.catalog.current();
        let manifest = catalog
            .server(&route.server)
            .ok_or_else(|| mcp_internal(format!("unknown task server `{}`", route.server)))?;
        project_call_tool_resource_uris(manifest, &mut result)?;
        result.meta = Some(veoveo_mcp_contract::related_task_meta(
            route.task_id.to_string(),
        ));
        serde_json::to_value(result)
            .map(GetTaskPayloadResult::new)
            .map_err(|error| mcp_internal(format!("failed to encode task result: {error}")))
    }

    pub(super) async fn handle_cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        let subject = self.authenticated(&context)?;
        if !self.client_allows_direct_task_adapter(&subject)? {
            return Err(mcp_invalid_params("unknown method"));
        }
        let route = self
            .authorize_canonical_task(&context, GatewayAction::TasksCancel, &request.task_id)
            .await?;
        let client = self
            .final_task_client(&route.server, &route.subject)
            .await?;
        client.cancel(route.task_id).await?;
        let task = client.get(route.task_id).await?;
        Ok(CancelTaskResult::new(project_task_from_detailed(&task)))
    }
}

fn is_terminal(task: &DetailedTask) -> bool {
    matches!(
        task,
        DetailedTask::Completed { .. }
            | DetailedTask::Failed { .. }
            | DetailedTask::Cancelled { .. }
    )
}
