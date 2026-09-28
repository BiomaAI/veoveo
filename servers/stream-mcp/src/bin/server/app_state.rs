use std::sync::Arc;

use veoveo_recording_reader::RecordingReader;
use veoveo_recording_video::runtime::VideoSourceLimits;
use veoveo_stream_mcp::{
    artifacts::ArtifactRepository, catalog::PipelineCatalog, contract::RunId,
    executor::StreamExecutor,
};
use veoveo_task_runtime::{TaskRuntime, TaskTransition};

use super::live::LiveSessionManager;

pub(super) struct AppState {
    pub(super) live_app: veoveo_mcp_apps_extension::AppHtml,
    pub(super) tasks: TaskRuntime,
    pub(super) artifacts: ArtifactRepository,
    pub(super) recordings: Arc<RecordingReader>,
    pub(super) catalog: Arc<PipelineCatalog>,
    pub(super) executor: StreamExecutor,
    pub(super) source_limits: VideoSourceLimits,
    pub(super) max_artifact_bytes: u64,
    pub(super) max_inline_resource_bytes: u64,
    pub(super) work_slots: Arc<tokio::sync::Semaphore>,
    pub(super) live: Arc<LiveSessionManager>,
}

pub(super) async fn update_task(state: &AppState, task_id: RunId, transition: TaskTransition) {
    let transition = if state
        .tasks
        .is_cancel_requested(&task_id.to_string())
        .await
        .unwrap_or(false)
    {
        TaskTransition::Cancelled
    } else {
        transition
    };
    if let Err(error) = state
        .tasks
        .transition(&task_id.to_string(), transition)
        .await
    {
        tracing::warn!(%task_id, "failed to transition durable stream task: {error}");
    }
}
