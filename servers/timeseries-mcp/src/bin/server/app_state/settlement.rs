//! Timeseries final outcomes honor committed cancellation through shared Resume settlement.
use veoveo_task_runtime::{
    ResumeCancellationPolicy, TaskError, TaskRuntime, TaskSnapshot, TaskTransition,
};
use veoveo_types::TaskId;

pub(super) async fn update(
    runtime: &TaskRuntime,
    id: TaskId,
    transition: TaskTransition,
) -> Result<TaskSnapshot, TaskError> {
    runtime
        .transition_resumable(
            id,
            transition,
            ResumeCancellationPolicy::CancellationWins,
            None,
        )
        .await
}

#[cfg(test)]
pub(super) async fn settle(
    runtime: &TaskRuntime,
    current: &TaskSnapshot,
    transition: TaskTransition,
) -> Result<TaskSnapshot, TaskError> {
    runtime
        .transition_resumable_if_current(
            current,
            transition,
            ResumeCancellationPolicy::CancellationWins,
            None,
        )
        .await
}
