use std::sync::Arc;

use veoveo_mcp_contract::SubscriptionHub;
use veoveo_perception_mcp::{
    artifacts::ArtifactRepository, catalog::PipelineCatalog, executor::DeepStreamExecutor,
    source::VideoSourceLimits,
};
use veoveo_recording_mcp::RecordingService;
use veoveo_task_runtime::{TaskRuntime, TaskTransition};

pub(super) struct AppState {
    pub(super) tasks: TaskRuntime,
    pub(super) artifacts: ArtifactRepository,
    pub(super) recordings: Arc<RecordingService>,
    pub(super) catalog: Arc<PipelineCatalog>,
    pub(super) executor: DeepStreamExecutor,
    pub(super) source_limits: VideoSourceLimits,
    pub(super) max_artifact_bytes: u64,
    pub(super) max_inline_resource_bytes: u64,
    pub(super) work_slots: Arc<tokio::sync::Semaphore>,
    pub(super) subscribers: SubscriptionHub,
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
    if let Err(error) = state.tasks.transition(task_id, transition).await {
        tracing::warn!(
            task_id,
            "failed to transition durable perception task: {error}"
        );
    }
    state
        .subscribers
        .notify_resource_updated(veoveo_perception_mcp::uris::analysis_uri(task_id))
        .await;
    state
        .subscribers
        .notify_resource_updated(veoveo_perception_mcp::uris::results_uri(task_id))
        .await;
}
