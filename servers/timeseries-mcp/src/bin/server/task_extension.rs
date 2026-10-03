//! Official durable Tasks handlers for the timeseries server.

use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        CallToolRequestParams, CreateTaskResult, GetTaskParams, GetTaskResult, UpdateTaskParams,
    },
    service::RequestContext,
};
use veoveo_mcp_contract::{
    GatewayInternalIdentity, PlaneCaller,
    hosting::{forwarded_bearer, gateway_identity},
};
use veoveo_task_runtime::{
    DurableTaskService, DurableTaskSubscription, cancel_durable_task, get_durable_task,
    retention_pins, subscribe_durable_tasks, task_seed, update_durable_task,
};
use veoveo_timeseries_mcp::contract::TimeseriesForecastRequest;

use super::{app_state::AppState, ownership::runtime_owner, start_forecast_task};

#[derive(Clone)]
pub(super) struct TimeseriesTaskService {
    state: Arc<AppState>,
}

impl TimeseriesTaskService {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
    plane: PlaneCaller,
}

impl DurableTaskService for TimeseriesTaskService {
    type Caller = AuthenticatedCaller;

    fn authenticate(&self, context: &RequestContext<RoleServer>) -> Result<Self::Caller, McpError> {
        authenticated_caller(context)
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: CallToolRequestParams,
    ) -> Result<Option<CreateTaskResult>, McpError> {
        if request.name != "forecast" {
            return Ok(None);
        }
        let args: TimeseriesForecastRequest = serde_json::from_value(serde_json::Value::Object(
            request.arguments.unwrap_or_default(),
        ))
        .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let snapshot = start_forecast_task(
            self.state.clone(),
            caller.identity.clone(),
            caller.plane.clone(),
            args,
            None,
            retention_pins(request.meta.as_ref())?,
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
        get_durable_task(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            request,
        )
        .await
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: UpdateTaskParams,
    ) -> Result<(), McpError> {
        update_durable_task(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            request,
        )
        .await
    }

    async fn cancel_task(&self, caller: &Self::Caller, task_id: String) -> Result<(), McpError> {
        cancel_durable_task(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            task_id,
        )
        .await
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<String>,
    ) -> Result<DurableTaskSubscription, McpError> {
        subscribe_durable_tasks(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            task_ids,
        )
        .await
    }
}

fn authenticated_caller(
    context: &RequestContext<RoleServer>,
) -> Result<AuthenticatedCaller, McpError> {
    let identity = gateway_identity(context)?;
    Ok(AuthenticatedCaller {
        plane: PlaneCaller::from_gateway(identity.clone(), forwarded_bearer(context)?),
        identity,
    })
}
