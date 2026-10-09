use veoveo_frames_mcp::{artifacts::ArtifactRepository, state::FramesState};
use veoveo_task_runtime::{TaskRuntime, TaskTransition};
use veoveo_types::TaskId;

pub(super) struct AppState {
    pub(super) tasks: TaskRuntime,
    pub(super) frames: FramesState,
    pub(super) artifacts: ArtifactRepository,
    pub(super) max_artifact_bytes: u64,
    pub(super) subscriptions: veoveo_mcp_contract::SubscriptionHub,
}

pub(super) async fn update_task(state: &AppState, task_id: TaskId, transition: TaskTransition) {
    if let Err(error) = state
        .tasks
        .transition_resumable(
            task_id,
            transition,
            veoveo_task_runtime::ResumeCancellationPolicy::PreserveFailure,
            None,
        )
        .await
    {
        tracing::warn!(%task_id, "Frames task update failed: {error}");
    }
}

pub(super) async fn publish_task(
    state: &AppState,
    task_id: TaskId,
    transition: TaskTransition,
    stop: &tokio_util::sync::CancellationToken,
) {
    if let Err(error) = state
        .tasks
        .transition_resumable(
            task_id,
            transition,
            veoveo_task_runtime::ResumeCancellationPolicy::PreserveFailure,
            Some(stop),
        )
        .await
    {
        tracing::warn!(%task_id, "Frames task publication failed: {error}");
    }
}

pub(super) async fn start_work(
    runtime: &TaskRuntime,
    task_id: TaskId,
) -> Result<bool, veoveo_task_runtime::TaskError> {
    Ok(runtime
        .transition_resumable(
            task_id,
            TaskTransition::Running {
                message: "running batch coordinate transform".into(),
                progress: 0.1,
            },
            veoveo_task_runtime::ResumeCancellationPolicy::PreserveFailure,
            None,
        )
        .await?
        .status
        == veoveo_task_runtime::TaskStatus::Running)
}

#[cfg(test)]
#[path = "app_state_tests.rs"]
mod tests;
