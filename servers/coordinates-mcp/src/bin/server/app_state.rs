use veoveo_coordinates_mcp::{artifacts::ArtifactRepository, state::CoordinatesState};
use veoveo_task_runtime::{TaskRuntime, TaskTransition};

pub(super) struct AppState {
    pub(super) tasks: TaskRuntime,
    pub(super) coordinates: CoordinatesState,
    pub(super) artifacts: ArtifactRepository,
    pub(super) max_artifact_bytes: u64,
}

pub(super) async fn update_task(state: &AppState, task_id: &str, transition: TaskTransition) {
    if let Err(error) = state.tasks.transition(task_id, transition).await {
        tracing::warn!(task_id, "coordinates task update failed: {error}");
    }
}
