use std::{sync::Arc, time::Duration};

use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};
use veoveo_media_mcp::state::ProviderCancellationOutcome;
use veoveo_platform_store::{ProviderJobState, TaskStatus};

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
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
    plane: PlaneCaller,
}

impl veoveo_task_runtime::DurableTaskService for MediaTaskExtension {
    type Caller = AuthenticatedCaller;

    fn authenticate(
        &self,
        context: &rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<Self::Caller, rmcp::ErrorData> {
        let parts = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("gateway identity missing", None))?;
        let identity = parts
            .extensions
            .get::<GatewayInternalIdentity>()
            .cloned()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("gateway identity missing", None))?;
        let bearer = parts
            .extensions
            .get::<ForwardedBearer>()
            .map(|bearer| bearer.0.clone())
            .ok_or_else(|| rmcp::ErrorData::invalid_request("forwarded bearer missing", None))?;
        Ok(AuthenticatedCaller {
            plane: caller_from(identity.clone(), bearer),
            identity,
        })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::CallToolRequestParams,
    ) -> Result<Option<rmcp::model::CreateTaskResult>, rmcp::ErrorData> {
        if request.name.as_ref() != "run" {
            return Ok(None);
        }
        let args: RunArgs = serde_json::from_value(serde_json::Value::Object(
            request.arguments.unwrap_or_default(),
        ))
        .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
        let snapshot = start_media_task(
            self.state.clone(),
            caller.identity.clone(),
            caller.plane.clone(),
            args,
            veoveo_task_runtime::retention_pins(request.meta.as_ref())?,
        )
        .await
        .map_err(|error| rmcp::ErrorData::internal_error(error, None))?;
        Ok(Some(rmcp::model::CreateTaskResult::new(
            veoveo_task_runtime::task_seed(&snapshot),
        )))
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::GetTaskParams,
    ) -> Result<rmcp::model::GetTaskResult, rmcp::ErrorData> {
        veoveo_task_runtime::get_durable_task(
            &self.state.tasks,
            &runtime_owner(&caller.identity),
            request,
        )
        .await
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::UpdateTaskParams,
    ) -> Result<(), rmcp::ErrorData> {
        veoveo_task_runtime::update_durable_task(
            &self.state.tasks,
            &runtime_owner(&caller.identity),
            request,
        )
        .await
    }

    async fn cancel_task(
        &self,
        caller: &Self::Caller,
        task_id: String,
    ) -> Result<(), rmcp::ErrorData> {
        let snapshot = veoveo_task_runtime::authorized_snapshot(
            &self.state.tasks,
            &runtime_owner(&caller.identity),
            &task_id,
        )
        .await?;
        let provider_job = self
            .state
            .durable
            .provider_job_for_task(&task_id)
            .await
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        let cancelled = self
            .state
            .tasks
            .cancel(&task_id)
            .await
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        if matches!(
            cancelled.status,
            TaskStatus::CancelRequested | TaskStatus::Cancelled
        ) && let Some(job) = provider_job
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
                .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
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
                .record_provider_cancellation(&cancelled, &job, outcome)
                .await
                .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
        }
        Ok(())
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<String>,
    ) -> Result<veoveo_task_runtime::DurableTaskSubscription, rmcp::ErrorData> {
        veoveo_task_runtime::subscribe_durable_tasks(
            &self.state.tasks,
            runtime_owner(&caller.identity),
            task_ids,
        )
        .await
    }
}
