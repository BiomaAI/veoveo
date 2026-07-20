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
    contract::{ReachableAreaRequest, RouteMatrixRequest, RouteRequest},
    server::auth::ForwardedBearer,
    state::MapApplication,
};

const SERVER_SLUG: &str = "map";
const ROUTE_TASK: &str = "route";
const ROUTE_MATRIX_TASK: &str = "route_matrix";
const REACHABLE_AREA_TASK: &str = "reachable_area";
const TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 3_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);

#[derive(Clone)]
pub(super) struct MapTaskExtension {
    state: Arc<MapApplication>,
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request", rename_all = "snake_case")]
enum MapTaskRequest {
    Route(RouteRequest),
    RouteMatrix(RouteMatrixRequest),
    ReachableArea(ReachableAreaRequest),
}

impl MapTaskExtension {
    pub(super) fn new(state: Arc<MapApplication>) -> Self {
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

impl TaskExtensionHandler for MapTaskExtension {
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
        let args = match request.name.as_str() {
            ROUTE_TASK => {
                require_scope(&caller.identity, "map:route")?;
                MapTaskRequest::Route(
                    serde_json::from_value(arguments)
                        .map_err(|error| AdapterError::invalid_params(error.to_string()))?,
                )
            }
            ROUTE_MATRIX_TASK => {
                require_scope(&caller.identity, "map:route_matrix")?;
                MapTaskRequest::RouteMatrix(
                    serde_json::from_value(arguments)
                        .map_err(|error| AdapterError::invalid_params(error.to_string()))?,
                )
            }
            REACHABLE_AREA_TASK => {
                require_scope(&caller.identity, "map:route")?;
                MapTaskRequest::ReachableArea(
                    serde_json::from_value(arguments)
                        .map_err(|error| AdapterError::invalid_params(error.to_string()))?,
                )
            }
            _ => return Ok(None),
        };
        let retention_pins = request.meta.task_retention_pin.into_iter().collect();
        let snapshot = start_map_task(
            self.state.clone(),
            caller.identity.clone(),
            args,
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
    state: Arc<MapApplication>,
    resumable: Vec<TaskSnapshot>,
) -> anyhow::Result<()> {
    for snapshot in resumable {
        if !matches!(
            snapshot.task_type.as_str(),
            ROUTE_TASK | ROUTE_MATRIX_TASK | REACHABLE_AREA_TASK
        ) {
            anyhow::bail!("unknown resumable Map task type `{}`", snapshot.task_type);
        }
        let request: MapTaskRequest = serde_json::from_value(snapshot.request.clone())?;
        if request.task_type() != snapshot.task_type {
            anyhow::bail!("Map task type does not match its persisted request");
        }
        if let Err(error) = schedule_map_task(state.clone(), snapshot, request).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(task_id, "another replica claimed recovered Map task");
                }
                _ => return Err(error),
            }
        }
    }
    Ok(())
}

async fn start_map_task(
    state: Arc<MapApplication>,
    identity: GatewayInternalIdentity,
    request: MapTaskRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> anyhow::Result<TaskSnapshot> {
    let task_type = request.task_type().to_owned();
    let created = state
        .tasks
        .create(CreateTask {
            task_id: TaskId::new(),
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type,
            request: serde_json::to_value(&request)?,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(TASK_TTL_MS),
            poll_interval_ms: Some(TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await?;
    schedule_map_task(state, created.snapshot, request).await
}

async fn schedule_map_task(
    state: Arc<MapApplication>,
    snapshot: TaskSnapshot,
    request: MapTaskRequest,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state.tasks.claim(&task_id, TASK_LEASE_DURATION).await?;
    let owner = snapshot.owner.clone();
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_map_task(
        state.clone(),
        task_id.clone(),
        owner,
        request,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn run_map_task(
    state: Arc<MapApplication>,
    task_id: String,
    owner: TaskOwner,
    request: MapTaskRequest,
    cancellation: CancellationToken,
) {
    let work = run_map_task_inner(
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
        tokio::select! {
            () = &mut work => break,
            _ = heartbeat.tick() => {
                if let Err(error) = state.tasks.renew_lease(&task_id, TASK_LEASE_DURATION).await {
                    tracing::warn!(task_id, "Map task lease heartbeat failed: {error}");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
}

async fn run_map_task_inner(
    state: Arc<MapApplication>,
    task_id: String,
    owner: TaskOwner,
    request: MapTaskRequest,
    cancellation: CancellationToken,
) {
    update_task(
        &state,
        &task_id,
        TaskTransition::Running {
            message: format!("calculating {}", request.description()),
            progress: 0.05,
        },
    )
    .await;
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    let result = match state.scope_from_task_owner(&owner).await {
        Ok(scope) => match request {
            MapTaskRequest::Route(request) => {
                state.routes.route(&scope, request).await.and_then(|route| {
                    tool_result(format!("planned route {}", route.route_id), &route)
                })
            }
            MapTaskRequest::RouteMatrix(request) => state
                .routes
                .route_matrix(&scope, request)
                .await
                .and_then(|matrix| {
                    tool_result(format!("calculated matrix {}", matrix.matrix_id), &matrix)
                }),
            MapTaskRequest::ReachableArea(request) => state
                .routes
                .reachable_area(&scope, request)
                .await
                .and_then(|area| {
                    tool_result(
                        format!("calculated reachable area {}", area.reachable_area_id),
                        &area,
                    )
                }),
        },
        Err(error) => Err(error),
    };
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    match result {
        Ok(tool_result) => match serde_json::to_value(tool_result) {
            Ok(result) => {
                update_task(
                    &state,
                    &task_id,
                    TaskTransition::Succeeded {
                        message: "Map calculation completed".to_owned(),
                        result,
                    },
                )
                .await;
            }
            Err(error) => fail_task(&state, &task_id, "result_serialization_failed", error).await,
        },
        Err(error) => fail_task(&state, &task_id, "map_calculation_failed", error).await,
    }
}

impl MapTaskRequest {
    fn task_type(&self) -> &'static str {
        match self {
            Self::Route(_) => ROUTE_TASK,
            Self::RouteMatrix(_) => ROUTE_MATRIX_TASK,
            Self::ReachableArea(_) => REACHABLE_AREA_TASK,
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::Route(_) => "logistics route",
            Self::RouteMatrix(_) => "logistics route matrix",
            Self::ReachableArea(_) => "land reachable area",
        }
    }
}

fn tool_result<T: Serialize>(text: String, value: &T) -> anyhow::Result<CallToolResult> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(value)?);
    Ok(result)
}

async fn fail_task(
    state: &MapApplication,
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

async fn update_task(state: &MapApplication, task_id: &str, transition: TaskTransition) {
    if let Err(error) = state.tasks.transition(task_id, transition).await {
        tracing::warn!(task_id, "Map task update failed: {error}");
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
