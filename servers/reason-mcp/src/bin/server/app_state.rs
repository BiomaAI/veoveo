use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use veoveo_reason_mcp::{
    artifacts::ArtifactRepository, catalog::PipelineCatalog, contract::AnalysisId,
    executor::ReasonExecutor,
};
use veoveo_recording_reader::RecordingReader;
use veoveo_recording_video::runtime::VideoSourceLimits;
use veoveo_task_runtime::{ResumeCancellationPolicy, TaskRuntime, TaskTransition};

pub(super) const CANCELLATION_POLICY: ResumeCancellationPolicy =
    ResumeCancellationPolicy::CancellationWins;

pub(super) struct AppState {
    pub(super) tasks: TaskRuntime,
    pub(super) finding_changes: veoveo_reason_mcp::knowledge::observe::FindingChanges,
    pub(super) artifacts: ArtifactRepository,
    pub(super) recordings: Arc<RecordingReader>,
    pub(super) catalog: Arc<PipelineCatalog>,
    pub(super) executor: ReasonExecutor,
    pub(super) source_limits: VideoSourceLimits,
    pub(super) max_artifact_bytes: u64,
    pub(super) max_inline_resource_bytes: u64,
    pub(super) max_grounding_bytes: u64,
    pub(super) work_slots: Arc<tokio::sync::Semaphore>,
}

pub(super) async fn update_task(
    state: &AppState,
    task_id: AnalysisId,
    transition: TaskTransition,
    stop: Option<&CancellationToken>,
) {
    if let Err(error) = state
        .tasks
        .transition_resumable(task_id.task_id(), transition, CANCELLATION_POLICY, stop)
        .await
    {
        tracing::warn!(%task_id, "reason Task settlement remains unresolved: {error}");
    }
}
