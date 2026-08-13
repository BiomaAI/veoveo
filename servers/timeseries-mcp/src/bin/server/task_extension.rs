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
    DurableTaskService, DurableTaskSubscription, TaskId, TaskRetentionPin, TaskSnapshot,
    cancel_durable_task, get_durable_task, project_snapshot, retention_pins,
    subscribe_durable_tasks, task_seed, update_durable_task,
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
        let snapshot = self.authorized_snapshot(&caller, &request.task_id).await?;
        project_snapshot(&self.state.tasks, snapshot)
            .await
            .map(GetTaskResult::new)
            .map_err(|error| McpError::internal_error(error.to_string(), None))
    }

    pub(super) async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller = authenticated_caller(context)?;
        self.authorized_snapshot(&caller, &request.task_id).await?;
        let responses = request
            .input_responses
            .into_iter()
            .map(|(key, value)| {
                value
                    .as_object()
                    .cloned()
                    .map(|value| (key, value.into_iter().collect()))
                    .ok_or_else(|| {
                        McpError::invalid_params("task input responses must be objects", None)
                    })
            })
            .collect::<Result<_, _>>()?;
        self.state
            .tasks
            .submit_input_responses(&request.task_id, responses)
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        Ok(())
    }

    pub(super) async fn cancel_task(
        &self,
        task_id: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller = authenticated_caller(context)?;
        self.authorized_snapshot(&caller, task_id).await?;
        self.state
            .tasks
            .cancel(task_id)
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        Ok(())
    }

    async fn authorized_snapshot(
        &self,
        caller: &AuthenticatedCaller,
        task_id: &str,
    ) -> Result<TaskSnapshot, McpError> {
        let task_id = task_id
            .parse::<TaskId>()
            .map_err(|_| McpError::invalid_params("unknown task id", None))?;
        let snapshot = self
            .state
            .tasks
            .get(&task_id.to_string())
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?
            .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
        let caller_owner = runtime_owner(&caller.identity);
        if snapshot.owner.allows(
            &caller_owner.principal_key,
            &caller_owner.profile,
            caller_owner.tenant_key.as_deref(),
            &caller_owner.data_labels,
        ) {
            Ok(snapshot)
        } else {
            Err(McpError::invalid_params("unknown task id", None))
        }
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

fn authenticated_caller(
    context: &RequestContext<RoleServer>,
) -> Result<AuthenticatedCaller, McpError> {
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
