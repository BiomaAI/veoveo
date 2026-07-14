use std::{collections::BTreeSet, sync::Arc, time::Duration};

use futures::StreamExt;
use rmcp::model::{CallToolResult, ContentBlock};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalKind};
use veoveo_mcp_task_extension::{
    AcknowledgeTaskResult, AdapterError, CancelTaskParams, CreateTaskResult, GetTaskParams,
    GetTaskResult, ProtocolTaskId, TaskExtensionHandler, TaskSubscription, ToolCallParams,
    UpdateTaskParams, project_snapshot, task_seed,
};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskError, TaskFailure, TaskId, TaskOwner, TaskRetentionPin,
    TaskSnapshot, TaskTransition,
};

use crate::{
    contract::{ExpandScheduleRequest, ValidateTimelineRequest},
    server::auth::ForwardedBearer,
    state::TimeApplication,
};

const SERVER_SLUG: &str = "time";
const EXPAND_SCHEDULE_TASK: &str = "expand_schedule";
const VALIDATE_TIMELINE_TASK: &str = "validate_timeline";
const TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 1_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);

#[derive(Clone)]
pub(super) struct TimeTaskExtension {
    state: Arc<TimeApplication>,
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "snake_case")]
enum TimeTaskRequest {
    ExpandSchedule(ExpandScheduleRequest),
    ValidateTimeline(ValidateTimelineRequest),
}

impl TimeTaskExtension {
    pub(super) fn new(state: Arc<TimeApplication>) -> Self {
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

impl TaskExtensionHandler for TimeTaskExtension {
    type Caller = AuthenticatedCaller;

    fn authenticate(
        &self,
        extensions: &axum::http::Extensions,
    ) -> Result<Self::Caller, AdapterError> {
        let identity = extensions
            .get::<GatewayInternalIdentity>()
            .cloned()
            .ok_or_else(|| AdapterError::unauthorized("gateway identity missing"))?;
        extensions
            .get::<ForwardedBearer>()
            .ok_or_else(|| AdapterError::unauthorized("forwarded bearer missing"))?;
        Ok(AuthenticatedCaller { identity })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: ToolCallParams,
    ) -> Result<Option<CreateTaskResult>, AdapterError> {
        let arguments = serde_json::Value::Object(request.arguments.into_iter().collect());
        let task = match request.name.as_str() {
            EXPAND_SCHEDULE_TASK => {
                require_scope(&caller.identity, "time:schedule")?;
                TimeTaskRequest::ExpandSchedule(
                    serde_json::from_value(arguments)
                        .map_err(|error| AdapterError::invalid_params(error.to_string()))?,
                )
            }
            VALIDATE_TIMELINE_TASK => {
                require_scope(&caller.identity, "time:timeline")?;
                TimeTaskRequest::ValidateTimeline(
                    serde_json::from_value(arguments)
                        .map_err(|error| AdapterError::invalid_params(error.to_string()))?,
                )
            }
            _ => return Ok(None),
        };
        let retention_pins = request.meta.task_retention_pin.into_iter().collect();
        let snapshot = start_time_task(
            self.state.clone(),
            caller.identity.clone(),
            task,
            retention_pins,
        )
        .await
        .map_err(|error| AdapterError::internal(error.to_string()))?;
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

pub(super) async fn recover_tasks(
    state: Arc<TimeApplication>,
    resumable: Vec<TaskSnapshot>,
) -> anyhow::Result<()> {
    for snapshot in resumable {
        if !matches!(
            snapshot.task_type.as_str(),
            EXPAND_SCHEDULE_TASK | VALIDATE_TIMELINE_TASK
        ) {
            anyhow::bail!("unknown resumable Time task type `{}`", snapshot.task_type);
        }
        let request: TimeTaskRequest = serde_json::from_value(snapshot.request.clone())?;
        if request.task_type() != snapshot.task_type {
            anyhow::bail!("Time task type does not match its persisted request");
        }
        if let Err(error) = schedule_time_task(state.clone(), snapshot, request).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(task_id, "another replica claimed recovered Time task")
                }
                _ => return Err(error),
            }
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
            task_type: request.task_type().to_owned(),
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
    let task_id = snapshot.task_id.to_string();
    let claimed = state.tasks.claim(&task_id, TASK_LEASE_DURATION).await?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_time_task(
        state.clone(),
        task_id.clone(),
        snapshot.owner,
        request,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn run_time_task(
    state: Arc<TimeApplication>,
    task_id: String,
    owner: TaskOwner,
    request: TimeTaskRequest,
    cancellation: CancellationToken,
) {
    let work = run_time_task_inner(
        state.clone(),
        task_id.clone(),
        owner,
        request,
        cancellation.clone(),
    );
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(TASK_LEASE_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    loop {
        tokio::select! { () = &mut work => break, _ = heartbeat.tick() => { if let Err(error) = state.tasks.renew_lease(&task_id, TASK_LEASE_DURATION).await { tracing::warn!(task_id, "Time task lease heartbeat failed: {error}"); cancellation.cancel(); break; } } }
    }
}

async fn run_time_task_inner(
    state: Arc<TimeApplication>,
    task_id: String,
    owner: TaskOwner,
    request: TimeTaskRequest,
    cancellation: CancellationToken,
) {
    update_task(
        &state,
        &task_id,
        TaskTransition::Running {
            message: request.description().to_owned(),
            progress: 0.05,
        },
    )
    .await;
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    let result = async {
        let scope = state.scope_from_task_owner(&owner).await?;
        let engine = state.engine(&scope).await?;
        match request {
            TimeTaskRequest::ExpandSchedule(request) => tool_result(
                "expanded operational schedule",
                &engine.expand_schedule(&request)?,
            ),
            TimeTaskRequest::ValidateTimeline(request) => tool_result(
                "validated mission timeline",
                &engine.validate_timeline(&request)?,
            ),
        }
    }
    .await;
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    match result {
        Ok(result) => match serde_json::to_value(result) {
            Ok(result) => {
                update_task(
                    &state,
                    &task_id,
                    TaskTransition::Succeeded {
                        message: "Temporal calculation completed".to_owned(),
                        result,
                    },
                )
                .await
            }
            Err(error) => fail_task(&state, &task_id, "result_serialization_failed", error).await,
        },
        Err(error) => fail_task(&state, &task_id, "temporal_calculation_failed", error).await,
    }
}

impl TimeTaskRequest {
    fn task_type(&self) -> &'static str {
        match self {
            Self::ExpandSchedule(_) => EXPAND_SCHEDULE_TASK,
            Self::ValidateTimeline(_) => VALIDATE_TIMELINE_TASK,
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
    task_id: &str,
    code: &str,
    error: impl std::fmt::Display,
) {
    update_task(
        state,
        task_id,
        TaskTransition::Failed(TaskFailure::new(code, error.to_string())),
    )
    .await;
}
async fn update_task(state: &TimeApplication, task_id: &str, transition: TaskTransition) {
    if let Err(error) = state.tasks.transition(task_id, transition).await {
        tracing::warn!(task_id, "Time task update failed: {error}");
    }
}

fn require_scope(identity: &GatewayInternalIdentity, required: &str) -> Result<(), AdapterError> {
    identity
        .principal
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
        .then_some(())
        .ok_or_else(|| AdapterError::unauthorized(format!("required scope `{required}` missing")))
}

fn runtime_owner(identity: &GatewayInternalIdentity) -> TaskOwner {
    TaskOwner {
        principal_key: identity.principal.id.to_string(),
        principal_kind: match identity.principal.kind {
            PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
            PrincipalKind::Service => veoveo_task_runtime::PrincipalKind::Service,
        },
        issuer: identity.principal.issuer.to_string(),
        subject: identity.principal.subject.to_string(),
        profile: identity.profile.to_string(),
        tenant_key: identity.principal.tenant.as_ref().map(ToString::to_string),
        data_labels: identity
            .principal
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}
