use std::{collections::BTreeSet, sync::Arc, time::Duration};

use rmcp::model::{CallToolResult, ContentBlock};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalKind};
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
pub(crate) struct TimeTaskExtension {
    state: Arc<TimeApplication>,
}

#[derive(Clone)]
pub(crate) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "snake_case")]
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
        let parts = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("gateway identity missing", None))?;
        let identity = parts
            .extensions
            .get::<GatewayInternalIdentity>()
            .cloned()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("gateway identity missing", None))?;
        parts
            .extensions
            .get::<ForwardedBearer>()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("forwarded bearer missing", None))?;
        Ok(AuthenticatedCaller { identity })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::CallToolRequestParams,
    ) -> Result<Option<rmcp::model::CreateTaskResult>, rmcp::ErrorData> {
        let arguments = serde_json::Value::Object(request.arguments.unwrap_or_default());
        let task = match request.name.as_ref() {
            EXPAND_SCHEDULE_TASK => {
                require_scope(&caller.identity, "time:schedule")
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                TimeTaskRequest::ExpandSchedule(Box::new(
                    serde_json::from_value(arguments).map_err(|error| {
                        rmcp::ErrorData::invalid_params(error.to_string(), None)
                    })?,
                ))
            }
            VALIDATE_TIMELINE_TASK => {
                require_scope(&caller.identity, "time:timeline")
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
                TimeTaskRequest::ValidateTimeline(
                    serde_json::from_value(arguments).map_err(|error| {
                        rmcp::ErrorData::invalid_params(error.to_string(), None)
                    })?,
                )
            }
            _ => return Ok(None),
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
        veoveo_task_runtime::cancel_durable_task(
            &self.state.tasks,
            &runtime_owner(&caller.identity),
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
            &self.state.tasks,
            runtime_owner(&caller.identity),
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

fn require_scope(
    identity: &GatewayInternalIdentity,
    required: &str,
) -> Result<(), rmcp::ErrorData> {
    identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
        .then_some(())
        .ok_or_else(|| {
            rmcp::ErrorData::invalid_request(format!("required scope `{required}` missing"), None)
        })
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
