use std::{collections::BTreeSet, sync::Arc, time::Duration};

use futures::StreamExt;
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
    contract::{CaptureFrameRequest, ViewRecord},
    mcp::frame_tool_result,
    server::{AppState, SERVER_SLUG, auth::ForwardedBearer},
    uris,
};

const CAPTURE_FRAME_TASK: &str = "capture_frame";
const TASK_TTL_MS: u64 = 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 1_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(180);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(super) struct ViewTaskExtension {
    state: Arc<AppState>,
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ViewCaptureTaskRequest {
    request: CaptureFrameRequest,
    view_snapshot: ViewRecord,
}

impl ViewTaskExtension {
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

impl TaskExtensionHandler for ViewTaskExtension {
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
        if request.name != CAPTURE_FRAME_TASK {
            return Ok(None);
        }
        require_scope(&caller.identity, "view:capture")?;
        let arguments = serde_json::Value::Object(request.arguments.into_iter().collect());
        let capture_request: CaptureFrameRequest = serde_json::from_value(arguments)
            .map_err(|error| AdapterError::invalid_params(error.to_string()))?;
        let owner = caller.identity.actor.id.to_string();
        let view_snapshot = self
            .state
            .views
            .capture_snapshot(&owner, &capture_request)
            .await
            .map_err(|error| AdapterError::invalid_params(error.to_string()))?;
        let task_request = ViewCaptureTaskRequest {
            request: capture_request,
            view_snapshot,
        };
        let retention_pins = request.meta.task_retention_pin.into_iter().collect();
        let snapshot = start_capture_task(
            self.state.clone(),
            caller.identity.clone(),
            task_request,
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
    state: Arc<AppState>,
    resumable: Vec<TaskSnapshot>,
) -> anyhow::Result<()> {
    for snapshot in resumable {
        if snapshot.task_type != CAPTURE_FRAME_TASK {
            anyhow::bail!("unknown resumable View task type `{}`", snapshot.task_type);
        }
        let request: ViewCaptureTaskRequest = serde_json::from_value(snapshot.request.clone())?;
        if let Err(error) = schedule_capture_task(state.clone(), snapshot, request, true).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(task_id, "another replica claimed recovered View task");
                }
                _ => return Err(error),
            }
        }
    }
    Ok(())
}

async fn start_capture_task(
    state: Arc<AppState>,
    identity: GatewayInternalIdentity,
    request: ViewCaptureTaskRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> anyhow::Result<TaskSnapshot> {
    let created = state
        .tasks
        .create(CreateTask {
            task_id: TaskId::new(),
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type: CAPTURE_FRAME_TASK.to_owned(),
            request: serde_json::to_value(&request)?,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(TASK_TTL_MS),
            poll_interval_ms: Some(TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await?;
    schedule_capture_task(state, created.snapshot, request, false).await
}

async fn schedule_capture_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: ViewCaptureTaskRequest,
    recovered: bool,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state.tasks.claim(&task_id, TASK_LEASE_DURATION).await?;
    let owner = snapshot.owner.clone();
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_capture_task(
        state.clone(),
        task_id.clone(),
        owner,
        request,
        recovered,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn run_capture_task(
    state: Arc<AppState>,
    task_id: String,
    owner: TaskOwner,
    request: ViewCaptureTaskRequest,
    recovered: bool,
    cancellation: CancellationToken,
) {
    let work = run_capture_task_inner(
        state.clone(),
        task_id.clone(),
        owner,
        request,
        recovered,
        cancellation.clone(),
    );
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(TASK_LEASE_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            () = &mut work => break,
            _ = heartbeat.tick() => {
                if let Err(error) = state.tasks.renew_lease(&task_id, TASK_LEASE_DURATION).await {
                    tracing::warn!(task_id, "View task lease heartbeat failed: {error}");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
}

async fn run_capture_task_inner(
    state: Arc<AppState>,
    task_id: String,
    owner: TaskOwner,
    request: ViewCaptureTaskRequest,
    recovered: bool,
    cancellation: CancellationToken,
) {
    update_task(
        &state,
        &task_id,
        TaskTransition::Running {
            message: "selecting and loading visible 3D tiles".to_owned(),
            progress: 0.05,
        },
    )
    .await;
    let permit = tokio::select! {
        () = cancellation.cancelled() => {
            update_task(&state, &task_id, TaskTransition::Cancelled).await;
            return;
        }
        permit = state.captures.acquire() => match permit {
            Ok(permit) => permit,
            Err(_) => {
                fail_task(&state, &task_id, "capture_scheduler_closed", "capture scheduler closed").await;
                return;
            }
        }
    };
    let result = if recovered {
        state
            .views
            .capture_recoverable_frame(
                &owner.principal_key,
                request.view_snapshot,
                request.request.policy,
                cancellation.clone(),
            )
            .await
    } else {
        state
            .views
            .capture_live_snapshot_frame(
                &owner.principal_key,
                request.view_snapshot,
                request.request.policy,
                cancellation.clone(),
            )
            .await
    };
    drop(permit);
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    match result {
        Ok(frame) => match frame_tool_result(&frame)
            .and_then(|result| Ok(serde_json::to_value(result)?))
        {
            Ok(result) => {
                state
                    .subscriptions
                    .notify_resource_updated(uris::FRAMES)
                    .await;
                update_task(
                    &state,
                    &task_id,
                    TaskTransition::Succeeded {
                        message: format!("captured {}", frame.record.frame_uri),
                        result,
                    },
                )
                .await;
            }
            Err(error) => fail_task(&state, &task_id, "result_serialization_failed", error).await,
        },
        Err(crate::state::ServiceError::Cancelled) => {
            update_task(&state, &task_id, TaskTransition::Cancelled).await;
        }
        Err(error) => fail_task(&state, &task_id, "view_capture_failed", error).await,
    }
}

async fn fail_task(state: &AppState, task_id: &str, code: &str, error: impl std::fmt::Display) {
    update_task(
        state,
        task_id,
        TaskTransition::Failed(TaskFailure::new(code, error.to_string())),
    )
    .await;
}

async fn update_task(state: &AppState, task_id: &str, transition: TaskTransition) {
    if let Err(error) = state.tasks.transition(task_id, transition).await {
        tracing::warn!(task_id, "View task update failed: {error}");
    }
}

fn require_scope(identity: &GatewayInternalIdentity, required: &str) -> Result<(), AdapterError> {
    identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
        .then_some(())
        .ok_or_else(|| AdapterError::unauthorized(format!("required scope `{required}` missing")))
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
