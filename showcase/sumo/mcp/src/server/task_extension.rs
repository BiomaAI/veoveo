use std::collections::BTreeSet;
use std::sync::Arc;

use futures::StreamExt;
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_mcp_task_extension::{
    AcknowledgeTaskResult, AdapterError, CancelTaskParams, CreateTaskResult, GetTaskParams,
    GetTaskResult, ProtocolTaskId, TaskExtensionHandler, TaskSubscription, ToolCallParams,
    UpdateTaskParams, project_snapshot, task_seed,
};
use veoveo_task_runtime::TaskSnapshot;

use crate::contract::{DurableOperation, OfflineOperationRequest, RunBatchRequest};

use super::ownership::{plane_caller, runtime_owner};
use super::state::AppState;
use super::task_worker::start_operation;

#[derive(Clone)]
pub(super) struct SumoTaskExtension {
    state: Arc<AppState>,
}

impl SumoTaskExtension {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    async fn authorized_snapshot(
        &self,
        caller: &AuthenticatedCaller,
        task_id: ProtocolTaskId,
    ) -> Result<TaskSnapshot, AdapterError> {
        let snapshot = self
            .state
            .tasks
            .get(&task_id.to_string())
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?
            .ok_or_else(|| AdapterError::invalid_params("unknown task id"))?;
        let owner = runtime_owner(&caller.identity);
        if snapshot.owner.allows(
            &owner.principal_key,
            &owner.profile,
            owner.tenant_key.as_deref(),
            &owner.data_labels,
        ) {
            Ok(snapshot)
        } else {
            Err(AdapterError::invalid_params("unknown task id"))
        }
    }
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
    plane: PlaneCaller,
}

impl TaskExtensionHandler for SumoTaskExtension {
    type Caller = AuthenticatedCaller;

    fn authenticate(
        &self,
        extensions: &axum::http::Extensions,
    ) -> Result<Self::Caller, AdapterError> {
        let identity = extensions
            .get::<GatewayInternalIdentity>()
            .cloned()
            .ok_or_else(|| AdapterError::unauthorized("gateway identity missing"))?;
        let bearer = extensions
            .get::<super::auth::ForwardedBearer>()
            .map(|bearer| bearer.0.clone())
            .ok_or_else(|| AdapterError::unauthorized("forwarded bearer missing"))?;
        Ok(AuthenticatedCaller {
            plane: plane_caller(identity.clone(), bearer),
            identity,
        })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: ToolCallParams,
    ) -> Result<Option<CreateTaskResult>, AdapterError> {
        let arguments = serde_json::Value::Object(request.arguments.into_iter().collect());
        let operation = match request.name.as_str() {
            "run_batch" => DurableOperation::RunBatch(
                serde_json::from_value::<RunBatchRequest>(arguments)
                    .map_err(|error| AdapterError::invalid_params(error.to_string()))?,
            ),
            "generate_network" => DurableOperation::GenerateNetwork(
                serde_json::from_value::<OfflineOperationRequest>(arguments)
                    .map_err(|error| AdapterError::invalid_params(error.to_string()))?,
            ),
            "compute_routes" => DurableOperation::ComputeRoutes(
                serde_json::from_value::<OfflineOperationRequest>(arguments)
                    .map_err(|error| AdapterError::invalid_params(error.to_string()))?,
            ),
            "optimize_signals" => DurableOperation::OptimizeSignals(
                serde_json::from_value::<OfflineOperationRequest>(arguments)
                    .map_err(|error| AdapterError::invalid_params(error.to_string()))?,
            ),
            _ => return Ok(None),
        };
        let snapshot = start_operation(
            self.state.clone(),
            caller.plane.clone(),
            operation,
            request.meta.task_retention_pin.into_iter().collect(),
        )
        .await
        .map_err(AdapterError::internal)?;
        Ok(Some(CreateTaskResult::new(task_seed(&snapshot))))
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: GetTaskParams,
    ) -> Result<GetTaskResult, AdapterError> {
        let snapshot = self.authorized_snapshot(caller, request.task_id).await?;
        let task = project_snapshot(&self.state.tasks, snapshot)
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        Ok(GetTaskResult::new(task))
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: UpdateTaskParams,
    ) -> Result<AcknowledgeTaskResult, AdapterError> {
        self.authorized_snapshot(caller, request.task_id).await?;
        self.state
            .tasks
            .submit_input_responses(&request.task_id.to_string(), request.input_responses)
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        Ok(AcknowledgeTaskResult::complete())
    }

    async fn cancel_task(
        &self,
        caller: &Self::Caller,
        request: CancelTaskParams,
    ) -> Result<AcknowledgeTaskResult, AdapterError> {
        self.authorized_snapshot(caller, request.task_id).await?;
        self.state
            .tasks
            .cancel(&request.task_id.to_string())
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        Ok(AcknowledgeTaskResult::complete())
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<ProtocolTaskId>,
    ) -> Result<TaskSubscription, AdapterError> {
        let updates = self
            .state
            .tasks
            .live_updates()
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        let mut accepted = Vec::new();
        for task_id in task_ids {
            if self.authorized_snapshot(caller, task_id).await.is_ok() {
                accepted.push(task_id);
            }
        }
        let accepted_set: BTreeSet<_> = accepted.iter().copied().collect();
        let runtime = self.state.tasks.clone();
        let owner = runtime_owner(&caller.identity);
        let stream = updates.filter_map(move |update| {
            let accepted = accepted_set.clone();
            let runtime = runtime.clone();
            let owner = owner.clone();
            async move {
                let snapshot = match update {
                    Ok(update) => update.snapshot,
                    Err(error) => return Some(Err(AdapterError::internal(error.to_string()))),
                };
                if !accepted.contains(&ProtocolTaskId::from(snapshot.task_id))
                    || !snapshot.owner.allows(
                        &owner.principal_key,
                        &owner.profile,
                        owner.tenant_key.as_deref(),
                        &owner.data_labels,
                    )
                {
                    return None;
                }
                Some(
                    project_snapshot(&runtime, snapshot)
                        .await
                        .map_err(|error| AdapterError::internal(error.to_string())),
                )
            }
        });
        Ok(TaskSubscription {
            accepted_task_ids: accepted,
            updates: Box::pin(stream),
        })
    }
}
