use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        CallToolRequestParams, CreateTaskResult, GetTaskParams, GetTaskResult, UpdateTaskParams,
    },
    service::RequestContext,
};
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_reason_mcp::contract::AnalyzeRecordingRequest;
use veoveo_task_runtime::{
    DurableTaskService, DurableTaskSubscription, cancel_durable_task, retention_pins, task_seed,
    update_durable_task,
};

use super::{
    app_state::AppState,
    ownership::runtime_owner,
    task_results,
    tasks::{ReasonTaskInput, start_reason_task},
};

#[derive(Clone)]
pub(super) struct ReasonTaskService {
    state: Arc<AppState>,
}

impl ReasonTaskService {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
    plane: PlaneCaller,
}

impl DurableTaskService for ReasonTaskService {
    type Caller = AuthenticatedCaller;

    fn authenticate(&self, context: &RequestContext<RoleServer>) -> Result<Self::Caller, McpError> {
        let identity = veoveo_mcp_contract::hosting::gateway_identity(context)?;
        Ok(AuthenticatedCaller {
            plane: PlaneCaller::from_gateway(
                identity.clone(),
                veoveo_mcp_contract::hosting::forwarded_bearer(context)?,
            ),
            identity,
        })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: CallToolRequestParams,
    ) -> Result<Option<CreateTaskResult>, McpError> {
        let arguments = serde_json::Value::Object(request.arguments.unwrap_or_default());
        let input = match request.name.as_ref() {
            "analyze_recording" => ReasonTaskInput::Analyze(
                serde_json::from_value::<AnalyzeRecordingRequest>(arguments)
                    .map_err(|error| McpError::invalid_params(error.to_string(), None))?,
            ),
            _ => return Ok(None),
        };
        let retention_pins = retention_pins(request.meta.as_ref())?;
        let snapshot = start_reason_task(
            self.state.clone(),
            caller.identity.clone(),
            caller.plane.clone(),
            input,
            None,
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
        task_results::get_task(&self.state.tasks, &runtime_owner(&caller.identity), request).await
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
        task_results::subscribe_tasks(&self.state.tasks, runtime_owner(&caller.identity), task_ids)
            .await
    }
}
