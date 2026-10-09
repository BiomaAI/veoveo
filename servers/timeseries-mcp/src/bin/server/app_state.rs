use veoveo_duckdb_runtime::HttpsSourcePolicy;
use veoveo_task_runtime::{TaskRuntime, TaskTransition};
use veoveo_timeseries_mcp::artifacts::ArtifactRepository;
use veoveo_types::TaskId;
#[path = "app_state/settlement.rs"]
mod settlement;
#[cfg(test)]
#[path = "app_state/tests.rs"]
mod tests;

pub(super) struct AppState {
    pub(super) tasks: TaskRuntime,
    pub(super) artifacts: ArtifactRepository,
    pub(super) source_policy: HttpsSourcePolicy,
    pub(super) max_artifact_bytes: u64,
}

pub(super) async fn update_task(state: &AppState, task_id: TaskId, transition: TaskTransition) {
    if let Err(err) = settlement::update(&state.tasks, task_id, transition).await {
        tracing::warn!(%task_id, "failed to transition durable task: {err}");
    }
}
