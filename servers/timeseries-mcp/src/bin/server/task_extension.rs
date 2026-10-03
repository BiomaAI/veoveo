//! Official durable Tasks handlers for the timeseries server.

use std::{collections::BTreeSet, sync::Arc};

use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        CallToolRequestParams, CreateTaskResult, GetTaskParams, GetTaskResult, UpdateTaskParams,
    },
    service::RequestContext,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_task_runtime::{
    DurableTaskService, DurableTaskSubscription, TaskRetentionPin, cancel_durable_task,
    get_durable_task, retention_pins, subscribe_durable_tasks, task_seed, update_durable_task,
};
use veoveo_timeseries_mcp::contract::TimeseriesForecastRequest;

use super::{
    TASK_RETENTION_PIN_META_KEY,
    app_state::AppState,
    internal_auth::ForwardedBearer,
    ownership::{caller_from, runtime_owner},
    start_forecast_task,
};

#[derive(Clone)]
pub(super) struct TimeseriesTaskService {
    state: Arc<AppState>,
}

impl TimeseriesTaskService {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    pub(super) async fn start_tool_task(
        &self,
        request: &CallToolRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<Option<CreateTaskResult>, McpError> {
        if request.name != "forecast" {
            return Ok(None);
        }
        let caller = authenticated_caller(context)?;
        let retention_pins = request
            .meta
            .as_ref()
            .and_then(|meta| meta.get(TASK_RETENTION_PIN_META_KEY))
            .cloned()
            .map(serde_json::from_value::<TaskRetentionPin>)
            .transpose()
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let args: TimeseriesForecastRequest = serde_json::from_value(serde_json::Value::Object(
            request.arguments.clone().unwrap_or_default(),
        ))
        .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let snapshot = start_forecast_task(
            self.state.clone(),
            caller.identity,
            caller.plane,
            args,
            None,
            retention_pins,
        )
        .await
        .map_err(|error| McpError::internal_error(error, None))?;
        Ok(Some(CreateTaskResult::new(task_seed(&snapshot))))
    }

    pub(super) async fn get_task(
        &self,
        request: GetTaskParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let caller = authenticated_caller(context)?;
        get_durable_task(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            request,
        )
        .await
    }

    pub(super) async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller = authenticated_caller(context)?;
        update_durable_task(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            request,
        )
        .await
    }

    pub(super) async fn cancel_task(
        &self,
        task_id: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller = authenticated_caller(context)?;
        cancel_durable_task(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            task_id.to_owned(),
        )
        .await
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
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    let identity = parts
        .extensions
        .get::<GatewayInternalIdentity>()
        .cloned()
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    let bearer = parts
        .extensions
        .get::<ForwardedBearer>()
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    Ok(AuthenticatedCaller {
        plane: caller_from(identity.clone(), bearer),
        identity,
    })
}
