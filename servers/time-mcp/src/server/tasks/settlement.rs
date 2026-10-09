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
    let current = snapshot(runtime, id).await?;
    settle(runtime, &current, transition, cancellation).await
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

fn selected_transition(current: &TaskSnapshot, transition: TaskTransition) -> TaskTransition {
    // Calculation failures preserve their real cause. Resume Tasks cannot publish
    // success after durable cancellation; runtime permits Failed or Cancelled.
    if current.status == TaskStatus::CancelRequested
        && !matches!(transition, TaskTransition::Failed(_))
    {
        TaskTransition::Cancelled
    } else {
        transition
    }
}

pub(super) async fn settle(
    runtime: &TaskRuntime,
    current: &TaskSnapshot,
    transition: TaskTransition,
    cancellation: &CancellationToken,
) -> Result<TaskSnapshot, TaskError> {
    if current.is_terminal() {
        return Ok(current.clone());
    }
    execution_owner(runtime, current)?;
    let selected = selected_transition(current, transition.clone());
    match dispatch(runtime, current, selected, cancellation).await {
        Ok(settled) => Ok(settled),
        Err(error @ (TaskError::Conflict(_) | TaskError::InvalidTransition { .. })) => {
            // One reconciliation for a cancellation/completion race, never a
            // retry of unrelated progress, lease failures or storage errors.
            let observed = snapshot(runtime, current.task_id).await?;
            if !same_identity(current, &observed) {
                return Err(error);
            }
            if observed.is_terminal() {
                return Ok(observed);
            }
            if observed.status != TaskStatus::CancelRequested {
                return Err(error);
            }
            execution_owner(runtime, &observed)?;
            dispatch(
                runtime,
                &observed,
                selected_transition(&observed, transition),
                cancellation,
            )
            .await
        }
        Err(error) => Err(error),
    }
}

/// The last owner checkpoint before entering the shared transition. Once that
/// future starts, its database awaits/CAS decide the outcome; local stop cannot
/// undo an already-dispatched transition.
pub(super) async fn dispatch(
    runtime: &TaskRuntime,
    current: &TaskSnapshot,
    transition: TaskTransition,
    cancellation: &CancellationToken,
) -> Result<TaskSnapshot, TaskError> {
    let locally_stopped = cancellation.is_cancelled();
    if matches!(transition, TaskTransition::Succeeded { .. }) && locally_stopped {
        // A local cancel may have committed after the selected snapshot. Read
        // once to distinguish durable cancel from shutdown-only stop.
        let observed = snapshot(runtime, current.task_id).await?;
        if !same_identity(current, &observed) {
            return Err(TaskError::Conflict(current.task_id.to_string()));
        }
        if observed.is_terminal() {
            return Ok(observed);
        }
        execution_owner(runtime, &observed)?;
        if observed.status == TaskStatus::CancelRequested {
            return runtime
                .transition_if_current(&observed, TaskTransition::Cancelled)
                .await;
        }
        return Ok(observed);
    }
    runtime.transition_if_current(current, transition).await
}

fn same_identity(left: &TaskSnapshot, right: &TaskSnapshot) -> bool {
    left.task_id == right.task_id
        && left.owner == right.owner
        && left.request == right.request
        && left.server == right.server
        && left.task_type == right.task_type
        && left.recovery_class == right.recovery_class
        && left.created_at == right.created_at
}
