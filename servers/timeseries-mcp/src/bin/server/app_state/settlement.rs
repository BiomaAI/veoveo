//! Timeseries final outcomes honor committed cancellation.
use veoveo_task_runtime::{TaskError, TaskRuntime, TaskSnapshot, TaskStatus, TaskTransition};
use veoveo_types::TaskId;

pub(super) async fn update(
    runtime: &TaskRuntime,
    id: TaskId,
    transition: TaskTransition,
) -> Result<TaskSnapshot, TaskError> {
    let current = runtime
        .get(id)
        .await?
        .ok_or_else(|| TaskError::NotFound(id.to_string()))?;
    settle(runtime, &current, transition).await
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
    if current.status == TaskStatus::CancelRequested {
        TaskTransition::Cancelled
    } else {
        transition
    }
}

pub(super) async fn settle(
    runtime: &TaskRuntime,
    current: &TaskSnapshot,
    transition: TaskTransition,
) -> Result<TaskSnapshot, TaskError> {
    if current.is_terminal() {
        return Ok(current.clone());
    }
    execution_owner(runtime, current)?;
    match runtime
        .transition_if_current(current, selected_transition(current, transition))
        .await
    {
        Ok(settled) => Ok(settled),
        Err(error @ (TaskError::Conflict(_) | TaskError::InvalidTransition { .. })) => {
            // A single reread handles a committed cancellation or completion.
            // Unrelated progress and lease/storage failures cannot authorize retry.
            let observed = runtime
                .get(current.task_id)
                .await?
                .ok_or_else(|| TaskError::NotFound(current.task_id.to_string()))?;
            if observed.owner != current.owner
                || observed.request != current.request
                || observed.server != current.server
                || observed.task_type != current.task_type
                || observed.recovery_class != current.recovery_class
                || observed.created_at != current.created_at
            {
                return Err(error);
            }
            if observed.is_terminal() {
                return Ok(observed);
            }
            if observed.status != TaskStatus::CancelRequested {
                return Err(error);
            }
            execution_owner(runtime, &observed)?;
            runtime
                .transition_if_current(&observed, TaskTransition::Cancelled)
                .await
        }
        Err(error) => Err(error),
    }
}
