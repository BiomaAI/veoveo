use rmcp::{
    model::{
        CancelTaskParams, ErrorData as McpError, GetTaskParams, GetTaskResult, UpdateTaskParams,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::GatewayAction;

use crate::mcp_support::{mcp_internal, mcp_invalid_params, upstream_error};

use super::{
    GatewayMcp,
    tools::{project_detailed_task_resource_uris, rewrite_detailed_task_id},
};

impl GatewayMcp {
    pub(super) async fn handle_get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let subject = self.authenticated(&context)?;
        if !self.client_allows_task_projection(&subject)? {
            return Err(mcp_invalid_params("unknown method"));
        }
        let canonical_task_id = request.task_id.clone();
        let route = self
            .authorize_canonical_task(&context, GatewayAction::TasksGet, &canonical_task_id)
            .await?;
        let upstream = self
            .upstream_with_tasks(&route.server, context.peer.clone(), &route.subject, true)
            .await?;
        let mut upstream_request = GetTaskParams::new(route.task_id);
        upstream_request.meta = request.meta;
        let mut result = upstream
            .peer
            .get_task(upstream_request)
            .await
            .map_err(upstream_error)?;
        let catalog = self.catalog.current();
        let manifest = catalog
            .server(&route.server)
            .ok_or_else(|| mcp_internal(format!("unknown task server `{}`", route.server)))?;
        project_detailed_task_resource_uris(manifest, &mut result.task)?;
        rewrite_detailed_task_id(&mut result.task, &canonical_task_id);
        Ok(result)
    }

    pub(super) async fn handle_update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let subject = self.authenticated(&context)?;
        if !self.client_allows_task_projection(&subject)? {
            return Err(mcp_invalid_params("unknown method"));
        }
        let route = self
            .authorize_canonical_task(&context, GatewayAction::TasksUpdate, &request.task_id)
            .await?;
        let upstream = self
            .upstream_with_tasks(&route.server, context.peer.clone(), &route.subject, true)
            .await?;
        let mut upstream_request = UpdateTaskParams::new(route.task_id, request.input_responses);
        upstream_request.meta = request.meta;
        upstream
            .peer
            .update_task(upstream_request)
            .await
            .map_err(upstream_error)
    }

    pub(super) async fn handle_cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let subject = self.authenticated(&context)?;
        if !self.client_allows_task_projection(&subject)? {
            return Err(mcp_invalid_params("unknown method"));
        }
        let route = self
            .authorize_canonical_task(&context, GatewayAction::TasksCancel, &request.task_id)
            .await?;
        let upstream = self
            .upstream_with_tasks(&route.server, context.peer.clone(), &route.subject, true)
            .await?;
        let mut upstream_request = CancelTaskParams::new(route.task_id);
        upstream_request.meta = request.meta;
        upstream
            .peer
            .cancel_task(upstream_request)
            .await
            .map_err(upstream_error)
    }
}
