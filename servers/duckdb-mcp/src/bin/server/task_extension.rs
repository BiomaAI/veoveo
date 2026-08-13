use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        CallToolRequestParams, CreateTaskResult, GetTaskParams, GetTaskResult, UpdateTaskParams,
    },
    service::RequestContext,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_task_runtime::{
    DurableTaskService, DurableTaskSubscription, cancel_durable_task, get_durable_task,
    retention_pins, subscribe_durable_tasks, task_seed, update_durable_task,
};

use super::{
    app_state::AppState,
    internal_auth::ForwardedBearer,
    ownership::{caller_from, runtime_owner},
    parse_task_args, start_duckdb_task,
};

#[derive(Clone)]
pub(super) struct DuckdbTaskService {
    state: Arc<AppState>,
}

impl DuckdbTaskService {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
    plane: PlaneCaller,
}

impl DurableTaskService for DuckdbTaskService {
    type Caller = AuthenticatedCaller;

    fn authenticate(&self, context: &RequestContext<RoleServer>) -> Result<Self::Caller, McpError> {
        let parts = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))?;
        let identity = parts
            .extensions
            .get::<GatewayInternalIdentity>()
            .cloned()
            .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))?;
        let bearer = parts
            .extensions
            .get::<ForwardedBearer>()
            .map(|bearer| bearer.0.clone())
            .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
        Ok(AuthenticatedCaller {
            plane: caller_from(identity.clone(), bearer),
            identity,
        })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: CallToolRequestParams,
    ) -> Result<Option<CreateTaskResult>, McpError> {
        if !matches!(
            request.name.as_ref(),
            "query" | "execute" | "ingest" | "export"
        ) {
            return Ok(None);
        }
        let retention_pins = retention_pins(request.meta.as_ref())?;
        let args = parse_task_args(
            request.name.as_ref(),
            serde_json::Value::Object(request.arguments.unwrap_or_default()),
        )
        .map_err(|error| McpError::invalid_params(error, None))?;
        let snapshot = start_duckdb_task(
            self.state.clone(),
            caller.identity.clone(),
            caller.plane.clone(),
            args,
            retention_pins,
        )
        .await
        .map_err(|error| McpError::internal_error(error, None))?;
        Ok(Some(CreateTaskResult::new(task_seed(&snapshot))))
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: GetTaskParams,
    ) -> Result<GetTaskResult, McpError> {
        get_durable_task(&self.state.tasks, &runtime_owner(&caller.identity), request).await
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: UpdateTaskParams,
    ) -> Result<(), McpError> {
        update_durable_task(&self.state.tasks, &runtime_owner(&caller.identity), request).await
    }

    async fn cancel_task(&self, caller: &Self::Caller, task_id: String) -> Result<(), McpError> {
        cancel_durable_task(&self.state.tasks, &runtime_owner(&caller.identity), task_id).await
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<String>,
    ) -> Result<DurableTaskSubscription, McpError> {
        subscribe_durable_tasks(&self.state.tasks, runtime_owner(&caller.identity), task_ids).await
    }
}
