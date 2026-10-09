use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use veoveo_recording_reader::RecordingReader;
use veoveo_recording_video::runtime::VideoSourceLimits;
use veoveo_stream_mcp::{
    artifacts::ArtifactRepository, catalog::PipelineCatalog, contract::RunId,
    executor::StreamExecutor,
};
use veoveo_task_runtime::{ResumeCancellationPolicy, TaskRuntime, TaskTransition};

pub(super) const CANCELLATION_POLICY: ResumeCancellationPolicy =
    ResumeCancellationPolicy::CancellationWins;

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

pub(super) async fn update_task(
    state: &AppState,
    task_id: RunId,
    transition: TaskTransition,
    stop: Option<&CancellationToken>,
) {
    if let Err(error) = state
        .tasks
        .transition_resumable(task_id.task_id(), transition, CANCELLATION_POLICY, stop)
        .await
    {
        tracing::warn!(%task_id, "stream Task settlement remains unresolved: {error}");
    }
}
