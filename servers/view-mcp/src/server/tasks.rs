use std::{collections::BTreeSet, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{GatewayInternalIdentity, PrincipalId, PrincipalKind};
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskError, TaskFailure, TaskId, TaskOwner, TaskRetentionPin,
    TaskSnapshot, TaskTransition,
};

use crate::{
    contract::CaptureFrameRequest,
    mcp::frame_tool_result,
    server::{AppState, SERVER_SLUG, auth::ForwardedBearer},
    state::{ResourceOwner, ViewCaptureSnapshot},
    uris,
};

const CAPTURE_FRAME_TASK: &str = "capture_frame";

const TASK_TTL_MS: u64 = 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 1_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(180);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(crate) struct ViewTaskExtension {
    state: Arc<AppState>,
}

#[derive(Clone)]
pub(crate) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ViewCaptureTaskRequest {
    request: CaptureFrameRequest,
    view_snapshot: ViewCaptureSnapshot,
}

impl ViewTaskExtension {
    pub(crate) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl veoveo_task_runtime::DurableTaskService for ViewTaskExtension {
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
        if request.name.as_ref() != CAPTURE_FRAME_TASK {
            return Ok(None);
        }
        require_scope(&caller.identity, "view:capture")
            .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
        let capture_request: CaptureFrameRequest = serde_json::from_value(
            serde_json::Value::Object(request.arguments.unwrap_or_default()),
        )
        .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
        let owner = ResourceOwner::from_identity(&caller.identity);
        let view_snapshot = self
            .state
            .views
            .capture_snapshot(&owner, &capture_request)
            .await
            .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
        let snapshot = start_capture_task(
            self.state.clone(),
            caller.identity.clone(),
            ViewCaptureTaskRequest {
                request: capture_request,
                view_snapshot,
            },
            veoveo_task_runtime::retention_pins(request.meta.as_ref())?,
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
    let resource_owner = match PrincipalId::new(owner.principal_key.clone()) {
        Ok(principal_id) => ResourceOwner {
            principal_id,
            work_context: owner.authority.work_context.clone(),
        },
        Err(error) => {
            drop(permit);
            fail_task(
                &state,
                &task_id,
                "invalid_task_owner",
                format!("stored task principal is invalid: {error}"),
            )
            .await;
            return;
        }
    };
    let result = if recovered {
        state
            .views
            .capture_recoverable_frame(
                &resource_owner,
                request.view_snapshot,
                request.request.scene_time,
                request.request.policy,
                cancellation.clone(),
            )
            .await
    } else {
        state
            .views
            .capture_live_snapshot_frame(
                &resource_owner,
                request.view_snapshot,
                request.request.scene_time,
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
