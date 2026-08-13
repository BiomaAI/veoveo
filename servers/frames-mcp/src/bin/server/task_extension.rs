use std::{collections::BTreeSet, sync::Arc};

use futures::StreamExt;
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{
        CallToolRequestParams, CreateTaskResult, GetTaskParams, GetTaskResult, UpdateTaskParams,
    },
    service::RequestContext,
};
use veoveo_frames_mcp::contract::BatchTransformRequest;
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_task_runtime::{
    DurableTaskService, DurableTaskSubscription, TaskId, TaskSnapshot, durable_input_responses,
    project_snapshot, retention_pins, task_seed,
};

use super::{
    app_state::AppState,
    internal_auth::ForwardedBearer,
    ownership::{caller_from, runtime_owner},
    start_batch_task,
};

#[derive(Clone)]
pub(super) struct FramesTaskService {
    state: Arc<AppState>,
}

impl FramesTaskService {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
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

impl DurableTaskService for FramesTaskService {
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
        let snapshot = self.authorized_snapshot(caller, &request.task_id).await?;
        project_snapshot(&self.state.tasks, snapshot)
            .await
            .map(GetTaskResult::new)
            .map_err(|error| McpError::internal_error(error.to_string(), None))
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: UpdateTaskParams,
    ) -> Result<(), McpError> {
        self.authorized_snapshot(caller, &request.task_id).await?;
        let task_id = request.task_id.clone();
        let responses = durable_input_responses(request)?;
        self.state
            .tasks
            .submit_input_responses(&task_id, responses)
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        Ok(())
    }

    async fn cancel_task(&self, caller: &Self::Caller, task_id: String) -> Result<(), McpError> {
        self.authorized_snapshot(caller, &task_id).await?;
        self.state
            .tasks
            .cancel(&task_id)
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        Ok(())
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<String>,
    ) -> Result<DurableTaskSubscription, McpError> {
        let updates = self
            .state
            .tasks
            .live_updates()
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        let mut accepted = Vec::new();
        for task_id in task_ids {
            if self.authorized_snapshot(caller, &task_id).await.is_ok() {
                accepted.push(task_id);
            }
        }
        let accepted_set: BTreeSet<_> = accepted.iter().cloned().collect();
        let runtime = self.state.tasks.clone();
        let caller_owner = runtime_owner(&caller.identity);
        let stream = updates.filter_map(move |update| {
            let accepted = accepted_set.clone();
            let runtime = runtime.clone();
            let caller_owner = caller_owner.clone();
            async move {
                let snapshot = match update {
                    Ok(update) => update.snapshot,
                    Err(error) => {
                        return Some(Err(McpError::internal_error(error.to_string(), None)));
                    }
                };
                if !accepted.contains(&snapshot.task_id.to_string())
                    || !snapshot.owner.allows(
                        &caller_owner.principal_key,
                        &caller_owner.profile,
                        caller_owner.tenant_key.as_deref(),
                        &caller_owner.data_labels,
                    )
                {
                    return None;
                }
                Some(
                    project_snapshot(&runtime, snapshot)
                        .await
                        .map_err(|error| McpError::internal_error(error.to_string(), None)),
                )
            }
        });
        Ok(DurableTaskSubscription {
            accepted_task_ids: accepted,
            updates: Box::pin(stream),
        })
    }
}
