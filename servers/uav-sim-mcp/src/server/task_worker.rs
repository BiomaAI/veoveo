use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use rmcp::model::{CallToolResult, ContentBlock};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::PlaneCaller;
use veoveo_task_runtime::{
    CreateTask, RecoveryClass, TaskFailure, TaskId, TaskPayloadState, TaskRetentionPin,
    TaskSnapshot, TaskTransition,
};

use crate::contract::{DurableOperation, DurableOperationResult, SessionId};
use crate::uris;

use super::ownership::runtime_owner;
use super::state::AppState;

const TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_POLL_INTERVAL_MS: u64 = 3_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);

pub(super) async fn start_operation(
    state: Arc<AppState>,
    caller: PlaneCaller,
    operation: DurableOperation,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let created = state
        .tasks
        .create(CreateTask {
            task_id: TaskId::new(),
            owner: runtime_owner(&caller.identity),
            server: "uav-sim".to_owned(),
            task_type: operation.task_type().to_owned(),
            request: serde_json::to_value(&operation).map_err(|error| error.to_string())?,
            recovery_class: recovery_class(&operation),
            idempotency_key: None,
            ttl_ms: Some(TASK_TTL_MS),
            poll_interval_ms: Some(TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|error| error.to_string())?;
    schedule_operation(state, created.snapshot, operation).await
}

pub(super) async fn resume_queued_operation(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
) -> Result<(), String> {
    let operation: DurableOperation =
        serde_json::from_value(snapshot.request.clone()).map_err(|error| error.to_string())?;
    schedule_operation(state, snapshot, operation)
        .await
        .map(|_| ())
}

async fn schedule_operation(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    operation: DurableOperation,
) -> Result<TaskSnapshot, String> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state
        .tasks
        .claim(&task_id, TASK_LEASE_DURATION)
        .await
        .map_err(|error| error.to_string())?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id.clone(),
        operation,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await
        .map_err(|error| error.to_string())?;
    Ok(claimed.snapshot)
}

async fn run_task(
    state: Arc<AppState>,
    task_id: String,
    operation: DurableOperation,
    cancellation: CancellationToken,
) {
    let session_id = operation_session(&operation).clone();
    let mission_uri = match &operation {
        DurableOperation::ExecuteMission(request) => Some(uris::mission(&request.mission_id)),
        _ => None,
    };
    let work = execute_operation(
        state.clone(),
        task_id.clone(),
        operation,
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
                    tracing::warn!(task_id, %error, "UAV simulation task lease heartbeat failed");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
    state
        .subscribers
        .notify_resource_updated(uris::session(&session_id))
        .await;
    state
        .subscribers
        .notify_resource_updated(uris::recordings(&session_id))
        .await;
    if let Some(uri) = mission_uri {
        state.subscribers.notify_resource_updated(uri).await;
    }
}

async fn execute_operation(
    state: Arc<AppState>,
    task_id: String,
    operation: DurableOperation,
    cancellation: CancellationToken,
) {
    let result = tokio::select! {
        result = async {
            state.adapter.execute(&operation).await
        } => result.map_err(|error| error.to_string()),
        () = cancellation.cancelled() => {
            transition(&state, &task_id, TaskTransition::Cancelled).await;
            return;
        }
    };
    if cancellation.is_cancelled() {
        transition(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    match result.and_then(operation_tool_result) {
        Ok(result) => match serde_json::to_value(result) {
            Ok(result) => {
                transition(
                    &state,
                    &task_id,
                    TaskTransition::Succeeded {
                        message: "completed".to_owned(),
                        result,
                    },
                )
                .await;
            }
            Err(error) => {
                transition(
                    &state,
                    &task_id,
                    TaskTransition::Failed(TaskFailure::new(
                        "result_serialization_failed",
                        error.to_string(),
                    )),
                )
                .await;
            }
        },
        Err(error) => {
            tracing::warn!(task_id, %error, "UAV simulation task failed");
            transition(
                &state,
                &task_id,
                TaskTransition::Failed(TaskFailure::new("uav_sim_operation_failed", error)),
            )
            .await;
        }
    }
}

fn operation_tool_result(result: DurableOperationResult) -> Result<CallToolResult, String> {
    match result {
        DurableOperationResult::RunScenario(value) => structured_result(
            format!("ran scenario for {:.3} seconds", value.elapsed_seconds),
            &value,
        ),
        DurableOperationResult::ExecuteMission(value) => {
            structured_result(format!("completed mission {}", value.mission_id), &value)
        }
        DurableOperationResult::CaptureDataset(value) => structured_result(
            format!(
                "captured {:.3} seconds of sensor data",
                value.elapsed_seconds
            ),
            &value,
        ),
    }
}

fn structured_result<T: Serialize>(message: String, value: &T) -> Result<CallToolResult, String> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(message)]);
    result.structured_content = Some(serde_json::to_value(value).map_err(|e| e.to_string())?);
    Ok(result)
}

fn operation_session(operation: &DurableOperation) -> &SessionId {
    match operation {
        DurableOperation::RunScenario(request) => &request.session_id,
        DurableOperation::ExecuteMission(request) => &request.session_id,
        DurableOperation::CaptureDataset(request) => &request.session_id,
    }
}

fn recovery_class(_operation: &DurableOperation) -> RecoveryClass {
    RecoveryClass::InterruptedIndeterminate
}

async fn transition(state: &AppState, task_id: &str, next: TaskTransition) {
    if let Err(error) = state.tasks.transition(task_id, next).await {
        tracing::warn!(task_id, %error, "UAV simulation task transition failed");
    }
}

pub(super) async fn await_result(
    state: &AppState,
    task_id: &str,
) -> Result<CallToolResult, rmcp::ErrorData> {
    match state
        .tasks
        .await_payload_state(task_id)
        .await
        .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None))?
    {
        TaskPayloadState::Completed(payload) => serde_json::from_value(payload)
            .map_err(|error| rmcp::ErrorData::internal_error(error.to_string(), None)),
        TaskPayloadState::Failed(error) => Err(rmcp::ErrorData::internal_error(
            error.message,
            error.details,
        )),
        TaskPayloadState::Cancelled => Err(rmcp::ErrorData::invalid_request(
            "UAV simulation task was cancelled",
            None,
        )),
        TaskPayloadState::Running => Err(rmcp::ErrorData::internal_error(
            "UAV simulation task wait ended while still running",
            None,
        )),
        TaskPayloadState::Unknown => Err(rmcp::ErrorData::internal_error(
            "UAV simulation task disappeared before completion",
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{CaptureDatasetRequest, SessionId};

    #[test]
    fn every_live_operation_is_indeterminate_after_interruption() {
        let operation = DurableOperation::CaptureDataset(CaptureDatasetRequest {
            session_id: SessionId::new("alpha").unwrap(),
            duration_seconds: 1.0,
            sensors: vec!["down-camera".to_owned()],
        });
        assert_eq!(operation.task_type(), "capture_dataset");
        assert_eq!(
            recovery_class(&operation),
            RecoveryClass::InterruptedIndeterminate
        );
    }
}
