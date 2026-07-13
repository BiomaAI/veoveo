use std::{collections::BTreeSet, sync::Arc, time::Duration};

use futures::StreamExt;
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_mcp_task_extension::{
    AcknowledgeTaskResult, AdapterError, CancelTaskParams, CreateTaskResult, GetTaskParams,
    GetTaskResult, ProtocolTaskId, TaskExtensionHandler, TaskSubscription, ToolCallParams,
    UpdateTaskParams, project_snapshot, task_seed,
};
use veoveo_media_mcp::state::ProviderCancellationOutcome;
use veoveo_platform_store::{ProviderJobState, TaskStatus};
use veoveo_task_runtime::TaskSnapshot;

use super::{
    AppState, RunArgs,
    internal_auth::ForwardedBearer,
    ownership::{caller_from, runtime_owner},
    start_media_task,
};

#[derive(Clone)]
pub(super) struct MediaTaskExtension {
    state: Arc<AppState>,
}

impl MediaTaskExtension {
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

impl TaskExtensionHandler for MediaTaskExtension {
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
            .get::<ForwardedBearer>()
            .map(|bearer| bearer.0.clone())
            .ok_or_else(|| AdapterError::unauthorized("forwarded bearer missing"))?;
        Ok(AuthenticatedCaller {
            plane: caller_from(identity.clone(), bearer),
            identity,
        })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: ToolCallParams,
    ) -> Result<Option<CreateTaskResult>, AdapterError> {
        if request.name != "run" {
            return Ok(None);
        }
        let args: RunArgs = serde_json::from_value(serde_json::Value::Object(
            request.arguments.into_iter().collect(),
        ))
        .map_err(|error| AdapterError::invalid_params(error.to_string()))?;
        let retention_pins = request.meta.task_retention_pin.into_iter().collect();
        let snapshot = start_media_task(
            self.state.clone(),
            caller.identity.clone(),
            caller.plane.clone(),
            args,
            retention_pins,
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
        let snapshot = self.authorized_snapshot(caller, request.task_id).await?;
        let provider_job = self
            .state
            .durable
            .provider_job_for_task(&request.task_id.to_string())
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        let cancelled = self
            .state
            .tasks
            .cancel(&request.task_id.to_string())
            .await
            .map_err(|error| AdapterError::internal(error.to_string()))?;
        let cancellation_applied = matches!(
            cancelled.status,
            TaskStatus::CancelRequested | TaskStatus::Cancelled
        );
        if cancellation_applied
            && let Some(job) = provider_job
            && matches!(
                job.state,
                ProviderJobState::Submitted
                    | ProviderJobState::Waiting
                    | ProviderJobState::CancelRequested
            )
        {
            let job = self
                .state
                .durable
                .record_provider_cancellation(
                    &snapshot,
                    &job,
                    ProviderCancellationOutcome::Requested,
                )
                .await
                .map_err(|error| AdapterError::internal(error.to_string()))?;
            let outcome = match tokio::time::timeout(
                Duration::from_secs(10),
                self.state
                    .provider
                    .request_cancellation(&job.external_job_id),
            )
            .await
            {
                Ok(Ok(receipt)) if receipt.deleted_count > 0 => {
                    ProviderCancellationOutcome::Accepted {
                        deleted_count: receipt.deleted_count,
                    }
                }
                Ok(Ok(receipt)) => ProviderCancellationOutcome::NotDeleted {
                    deleted_count: receipt.deleted_count,
                },
                Ok(Err(error)) => ProviderCancellationOutcome::Failed {
                    error: error.to_string(),
                },
                Err(_) => ProviderCancellationOutcome::Failed {
                    error: "provider cancellation request timed out".to_owned(),
                },
            };
            self.state
                .durable
                .record_provider_cancellation(&cancelled, &job, outcome.clone())
                .await
                .map_err(|error| AdapterError::internal(error.to_string()))?;
            match outcome {
                ProviderCancellationOutcome::Accepted { deleted_count } => tracing::info!(
                    task_id = %request.task_id,
                    provider_job_id = job.external_job_id,
                    deleted_count,
                    "provider acknowledged best-effort media cancellation"
                ),
                ProviderCancellationOutcome::NotDeleted { deleted_count } => tracing::warn!(
                    task_id = %request.task_id,
                    provider_job_id = job.external_job_id,
                    deleted_count,
                    "provider did not delete the cancelled media prediction; work or billing may continue"
                ),
                ProviderCancellationOutcome::Failed { error } => tracing::warn!(
                    task_id = %request.task_id,
                    provider_job_id = job.external_job_id,
                    error,
                    "best-effort provider cancellation failed; work or billing may continue"
                ),
                ProviderCancellationOutcome::Requested => unreachable!("terminal outcome built"),
            }
        }
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
        let caller_owner = runtime_owner(&caller.identity);
        let stream = updates.filter_map(move |update| {
            let accepted = accepted_set.clone();
            let runtime = runtime.clone();
            let caller_owner = caller_owner.clone();
            async move {
                let snapshot = match update {
                    Ok(update) => update.snapshot,
                    Err(error) => return Some(Err(AdapterError::internal(error.to_string()))),
                };
                if !accepted.contains(&ProtocolTaskId::from(snapshot.task_id))
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
