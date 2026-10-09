mod settlement;
#[cfg(test)]
mod tests;

use crate::contract::TimeTaskKind;
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use veoveo_types::TaskTypeDefinition;

use rmcp::model::{CallToolResult, ContentBlock};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalKind};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskFailure, TaskOwner, TaskRetentionPin, TaskSnapshot,
    TaskTransition,
};
use veoveo_types::TaskId;

use crate::{
    contract::{ExpandScheduleRequest, TimeScope, ValidateTimelineRequest},
    server::auth::require_scope,
    state::TimeApplication,
};

const SERVER_SLUG: &str = "time";

const TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 1_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);

#[derive(Clone)]
pub(crate) struct TimeTaskExtension {
    state: Arc<TimeApplication>,
}

#[derive(Clone)]
pub(crate) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum TimeTaskRequest {
    ExpandSchedule(Box<ExpandScheduleRequest>),
    ValidateTimeline(ValidateTimelineRequest),
}

impl TimeTaskExtension {
    pub(crate) fn new(state: Arc<TimeApplication>) -> Self {
        Self { state }
    }
}

impl veoveo_task_runtime::DurableTaskService for TimeTaskExtension {
    type Caller = AuthenticatedCaller;

    fn authenticate(
        &self,
        context: &rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<Self::Caller, rmcp::ErrorData> {
        let identity = veoveo_mcp_contract::hosting::gateway_identity(context)?;
        Ok(AuthenticatedCaller { identity })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::CallToolRequestParams,
    ) -> Result<Option<rmcp::model::CreateTaskResult>, rmcp::ErrorData> {
        let arguments = serde_json::Value::Object(request.arguments.unwrap_or_default());
        let task = match TimeTaskKind::from_wire_name(request.name.as_ref()) {
            Some(TimeTaskKind::ExpandSchedule) => {
                require_scope(&caller.identity.actor.scopes, TimeScope::Schedule)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                TimeTaskRequest::ExpandSchedule(Box::new(
                    serde_json::from_value(arguments).map_err(|error| {
                        rmcp::ErrorData::invalid_params(error.to_string(), None)
                    })?,
                ))
            }
            Some(TimeTaskKind::ValidateTimeline) => {
                require_scope(&caller.identity.actor.scopes, TimeScope::Timeline)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                TimeTaskRequest::ValidateTimeline(
                    serde_json::from_value(arguments).map_err(|error| {
                        rmcp::ErrorData::invalid_params(error.to_string(), None)
                    })?,
                )
            }
            None => return Ok(None),
        };
        let retention_pins = veoveo_task_runtime::retention_pins(request.meta.as_ref())?;
        let snapshot = start_time_task(
            self.state.clone(),
            caller.identity.clone(),
            task,
            retention_pins,
        )
        .await
        .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?;
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
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
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
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            request,
        )
        .await
    }

    async fn cancel_task(
        &self,
        caller: &Self::Caller,
        task_id: String,
    ) -> Result<(), rmcp::ErrorData> {
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
    ) -> Result<veoveo_task_runtime::DurableTaskSubscription, rmcp::ErrorData> {
        veoveo_task_runtime::subscribe_durable_tasks(
            &self.state.tasks.for_owner(&runtime_owner(&caller.identity)),
            task_ids,
        )
        .await
    }
}

pub(super) async fn recover_tasks(
    state: Arc<TimeApplication>,
    resumable: Vec<TaskSnapshot>,
) -> anyhow::Result<()> {
    for snapshot in resumable {
        if TimeTaskKind::from_name(&snapshot.task_type).is_none() {
            anyhow::bail!("unknown resumable Time task type `{}`", snapshot.task_type);
        }
        let request: TimeTaskRequest = serde_json::from_value(snapshot.request.clone())?;
        if request.task_type() != snapshot.task_type {
            anyhow::bail!("Time task type does not match its persisted request");
        }
        let admitted = snapshot.clone();
        if let Err(error) = schedule_time_task(state.clone(), snapshot, request).await {
            state
                .tasks
                .reconcile_recovery_claim(&admitted, error)
                .await?;
        }
    }
    Ok(())
}

async fn start_time_task(
    state: Arc<TimeApplication>,
    identity: GatewayInternalIdentity,
    request: TimeTaskRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> anyhow::Result<TaskSnapshot> {
    let created = state
        .tasks
        .create(CreateTask {
            task_id: TaskId::new(),
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type: request.task_type(),
            request: serde_json::to_value(&request)?,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(TASK_TTL_MS),
            poll_interval_ms: Some(TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await?;
    schedule_time_task(state, created.snapshot, request).await
}

async fn schedule_time_task(
    state: Arc<TimeApplication>,
    snapshot: TaskSnapshot,
    request: TimeTaskRequest,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id;
    let claimed = state.tasks.claim(task_id, TASK_LEASE_DURATION).await?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_time_task(
        state.clone(),
        task_id,
        snapshot.owner,
        request,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn run_time_task(
    state: Arc<TimeApplication>,
    task_id: TaskId,
    owner: TaskOwner,
    request: TimeTaskRequest,
    cancellation: CancellationToken,
) {
    let work = run_time_task_inner(state.clone(), task_id, owner, request, cancellation.clone());
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(TASK_LEASE_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    loop {
        tokio::select! { result = &mut work => {
            if let Err(error) = result {
                tracing::warn!(%task_id, "Time task execution did not settle: {error}");
            }
            break;
        }, _ = heartbeat.tick() => { if let Err(error) = state.tasks.renew_lease(task_id, TASK_LEASE_DURATION).await { tracing::warn!(%task_id, "Time task lease heartbeat failed: {error}"); cancellation.cancel(); break; } } }
    }
}

async fn run_time_task_inner(
    state: Arc<TimeApplication>,
    task_id: TaskId,
    owner: TaskOwner,
    request: TimeTaskRequest,
    cancellation: CancellationToken,
) -> anyhow::Result<()> {
    update_task(
        &state,
        task_id,
        TaskTransition::Running {
            message: request.description().to_owned(),
            progress: 0.05,
        },
        &cancellation,
    )
    .await?;
    if !settlement::continue_work(&state.tasks, task_id, &cancellation).await? {
        return Ok(());
    }
    let scope = match state.scope_from_task_owner(&owner).await {
        Ok(scope) => scope,
        Err(error) => {
            return fail_task(
                &state,
                task_id,
                "temporal_calculation_failed",
                error,
                &cancellation,
            )
            .await;
        }
    };
    if !settlement::continue_work(&state.tasks, task_id, &cancellation).await? {
        return Ok(());
    }
    let engine = match &request {
        TimeTaskRequest::ExpandSchedule(_) => state.engine(&scope).await,
        TimeTaskRequest::ValidateTimeline(request) => {
            state
                .engine_for_expressions(&scope, request.points.iter().map(|point| &point.at))
                .await
        }
    };
    let engine = match engine {
        Ok(engine) => engine,
        Err(error) => {
            return fail_task(
                &state,
                task_id,
                "temporal_calculation_failed",
                error,
                &cancellation,
            )
            .await;
        }
    };
    if !settlement::continue_work(&state.tasks, task_id, &cancellation).await? {
        return Ok(());
    }
    let result = match request {
        TimeTaskRequest::ExpandSchedule(request) => engine
            .expand_schedule(&request)
            .and_then(|output| tool_result("expanded operational schedule", &output)),
        TimeTaskRequest::ValidateTimeline(request) => engine
            .validate_timeline(&request)
            .and_then(|output| tool_result("validated mission timeline", &output)),
    };
    // Shutdown can cancel a local worker without a durable cancellation request.
    // Stop publication and let the retained lease follow Resume recovery.
    if settlement::local_stop(&state.tasks, task_id, &cancellation).await? {
        return Ok(());
    }
    match result {
        Ok(result) => {
            match veoveo_task_runtime::mcp_task_completion("Temporal calculation completed", result)
            {
                Ok(transition) => update_task(&state, task_id, transition, &cancellation).await,
                Err(error) => {
                    fail_task(
                        &state,
                        task_id,
                        "result_serialization_failed",
                        error,
                        &cancellation,
                    )
                    .await
                }
            }
        }
        Err(error) => {
            fail_task(
                &state,
                task_id,
                "temporal_calculation_failed",
                error,
                &cancellation,
            )
            .await
        }
    }
}

impl TimeTaskRequest {
    fn task_type(&self) -> veoveo_types::TaskTypeName {
        match self {
            Self::ExpandSchedule(_) => TimeTaskKind::ExpandSchedule.name(),
            Self::ValidateTimeline(_) => TimeTaskKind::ValidateTimeline.name(),
        }
    }
    fn description(&self) -> &'static str {
        match self {
            Self::ExpandSchedule(_) => "expanding operational calendar",
            Self::ValidateTimeline(_) => "validating mission timeline",
        }
    }
}

fn tool_result<T: Serialize>(text: &str, value: &T) -> anyhow::Result<CallToolResult> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(value)?);
    Ok(result)
}

async fn fail_task(
    state: &TimeApplication,
    task_id: TaskId,
    code: &str,
    error: impl std::fmt::Display,
    cancellation: &CancellationToken,
) -> anyhow::Result<()> {
    update_task(
        state,
        task_id,
        TaskTransition::Failed(TaskFailure::new(code, error.to_string())),
        cancellation,
    )
    .await
}
async fn update_task(
    state: &TimeApplication,
    task_id: TaskId,
    transition: TaskTransition,
    cancellation: &CancellationToken,
) -> anyhow::Result<()> {
    settlement::update(&state.tasks, task_id, transition, cancellation).await?;
    Ok(())
}

fn runtime_owner(identity: &GatewayInternalIdentity) -> TaskOwner {
    TaskOwner {
        principal_key: identity.actor.id.to_string(),
        principal_kind: match identity.actor.kind {
            PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            PrincipalKind::Service => veoveo_task_runtime::PrincipalKind::Service,
        },
        issuer: identity.actor.issuer.to_string(),
        subject: identity.actor.subject.to_string(),
        profile: identity.profile.to_string(),
        tenant_key: identity.actor.tenant.as_ref().map(ToString::to_string),
        data_labels: identity
            .actor
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        authority: identity.authority.clone(),
    }
}
