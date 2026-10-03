use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        CallToolRequestParams, CreateTaskResult, GetTaskParams, GetTaskResult, UpdateTaskParams,
    },
    service::RequestContext,
};
use veoveo_frames_mcp::contract::BatchTransformRequest;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, PlaneCaller,
    hosting::{forwarded_bearer, gateway_identity},
};
use veoveo_task_runtime::{DurableTaskService, DurableTaskSubscription, retention_pins, task_seed};

use super::{app_state::AppState, ownership::runtime_owner, start_batch_task};

#[derive(Clone)]
pub(super) struct FramesTaskService {
    state: Arc<AppState>,
}

impl FramesTaskService {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
    plane: PlaneCaller,
}

impl DurableTaskService for FramesTaskService {
    type Caller = AuthenticatedCaller;

    fn authenticate(&self, context: &RequestContext<RoleServer>) -> Result<Self::Caller, McpError> {
        let identity = gateway_identity(context)?;
        Ok(AuthenticatedCaller {
            plane: PlaneCaller::from_gateway(identity.clone(), forwarded_bearer(context)?),
            identity,
        })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: CallToolRequestParams,
    ) -> Result<Option<CreateTaskResult>, McpError> {
        if request.name != "batch_transform" {
            return Ok(None);
        }
        let retention_pins = retention_pins(request.meta.as_ref())?;
        let args: BatchTransformRequest = serde_json::from_value(serde_json::Value::Object(
            request.arguments.unwrap_or_default(),
        ))
        .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let snapshot = start_batch_task(
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
        veoveo_task_runtime::get_durable_task(
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
        veoveo_task_runtime::update_durable_task(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            request,
        )
        .await
    }

    async fn cancel_task(&self, caller: &Self::Caller, task_id: String) -> Result<(), McpError> {
        veoveo_task_runtime::cancel_durable_task(
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
        veoveo_task_runtime::subscribe_durable_tasks(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            task_ids,
        )
        .await
    }
}
