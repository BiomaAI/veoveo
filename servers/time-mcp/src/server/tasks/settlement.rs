//! Time-owned cancellation checkpoints and finite calculation settlement.
use tokio_util::sync::CancellationToken;
use veoveo_platform_store::TaskStatus;
use veoveo_task_runtime::{TaskError, TaskRuntime, TaskSnapshot, TaskTransition};
use veoveo_types::TaskId;

pub(super) async fn update(
    runtime: &TaskRuntime,
    id: TaskId,
    transition: TaskTransition,
    cancellation: &CancellationToken,
) -> Result<TaskSnapshot, TaskError> {
    runtime
        .transition_resumable(
            id,
            transition,
            veoveo_task_runtime::ResumeCancellationPolicy::PreserveFailure,
            Some(cancellation),
        )
        .await
}

/// Observe durable cancellation at existing async calculation checkpoints.
/// A remote replica need not share this worker's local cancellation token.
pub(super) async fn checkpoint(runtime: &TaskRuntime, id: TaskId) -> Result<bool, TaskError> {
    let current = snapshot(runtime, id).await?;
    if current.is_terminal() {
        return Ok(false);
    }
    execution_owner(runtime, &current)?;
    if current.status == TaskStatus::CancelRequested {
        dispatch(
            runtime,
            &current,
            TaskTransition::Cancelled,
            &CancellationToken::new(),
        )
        .await?;
        return Ok(false);
    }
    Ok(true)
}

/// Local shutdown stops work without inventing a durable cancel request.
pub(super) async fn continue_work(
    runtime: &TaskRuntime,
    id: TaskId,
    cancellation: &CancellationToken,
) -> Result<bool, TaskError> {
    Ok(checkpoint(runtime, id).await? && !cancellation.is_cancelled())
}

pub(super) async fn local_stop(
    runtime: &TaskRuntime,
    id: TaskId,
    cancellation: &CancellationToken,
) -> Result<bool, TaskError> {
    if !cancellation.is_cancelled() {
        return Ok(false);
    }
    Ok(snapshot(runtime, id).await?.status != TaskStatus::CancelRequested)
}

async fn snapshot(runtime: &TaskRuntime, id: TaskId) -> Result<TaskSnapshot, TaskError> {
    runtime
        .get(id)
        .await?
        .ok_or_else(|| TaskError::NotFound(id.to_string()))
}

fn execution_owner(runtime: &TaskRuntime, current: &TaskSnapshot) -> Result<(), TaskError> {
    if current.lease_owner.as_deref() != Some(runtime.worker_id())
        || current
            .lease_expires_at
            .is_none_or(|expiry| expiry <= chrono::Utc::now())
    {
        return Err(TaskError::LeaseHeld(current.task_id.to_string()));
    }
    Ok(())
}

pub(super) async fn settle(
    runtime: &TaskRuntime,
    current: &TaskSnapshot,
    transition: TaskTransition,
    cancellation: &CancellationToken,
) -> Result<TaskSnapshot, TaskError> {
    runtime
        .transition_resumable_if_current(
            current,
            transition,
            veoveo_task_runtime::ResumeCancellationPolicy::PreserveFailure,
            Some(cancellation),
        )
        .await
}

/// Preserve Time's final selected-snapshot dispatch checkpoint.
pub(super) async fn dispatch(
    runtime: &TaskRuntime,
    current: &TaskSnapshot,
    transition: TaskTransition,
    cancellation: &CancellationToken,
) -> Result<TaskSnapshot, TaskError> {
    settle(runtime, current, transition, cancellation).await
}
