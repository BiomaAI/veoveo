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
    if let Err(error) = state.tasks.transition(task_id, transition).await {
        tracing::warn!(%task_id, "Frames task update failed: {error}");
    }
}
