use veoveo_duckdb_runtime::HttpsSourcePolicy;
use veoveo_task_runtime::{TaskRuntime, TaskTransition};
use veoveo_timeseries_mcp::artifacts::ArtifactRepository;

pub(super) struct AppState {
    pub(super) tasks: TaskRuntime,
    pub(super) artifacts: ArtifactRepository,
    pub(super) source_policy: HttpsSourcePolicy,
    pub(super) max_artifact_bytes: u64,
}

pub(super) async fn update_task(state: &AppState, task_id: &str, transition: TaskTransition) {
    let transition = if state
        .tasks
        .is_cancel_requested(task_id)
        .await
        .unwrap_or(false)
    {
        TaskTransition::Cancelled
    } else {
        transition
    };
    if let Err(err) = state.tasks.transition(task_id, transition).await {
        tracing::warn!(task_id, "failed to transition durable task: {err}");
    }
}
