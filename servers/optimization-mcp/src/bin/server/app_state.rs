use futures::StreamExt;
use veoveo_mcp_contract::{ResourceListObservers, SubscriptionHub};
use veoveo_optimization_mcp::{
    artifacts::ArtifactRepository,
    executor::{ExecutorClient, ExecutorHealth},
    problem_store::ProblemStore,
};
use veoveo_platform_store::PlatformTable;
use veoveo_task_runtime::{TaskError, TaskRuntime, TaskSnapshot, TaskTransition};
use veoveo_types::TaskId;

pub(super) struct AppState {
    pub(super) tasks: TaskRuntime,
    pub(super) artifacts: ArtifactRepository,
    pub(super) executor: ExecutorClient,
    pub(super) executor_health: ExecutorHealth,
    pub(super) executor_slot: std::sync::Arc<tokio::sync::Semaphore>,
    pub(super) problem_store: ProblemStore,
    pub(super) subscriptions: std::sync::Arc<SubscriptionHub>,
    pub(super) resource_observers: std::sync::Arc<ResourceListObservers>,
    pub(super) max_artifact_bytes: u64,
    pub(super) max_executor_frame_bytes: u64,
}

pub(super) fn spawn_resource_observer(
    state: std::sync::Arc<AppState>,
    cancellation: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut changes = state
            .tasks
            .platform_store()
            .resource_changes(vec![PlatformTable::Task, PlatformTable::DomainUsage]);
        loop {
            tokio::select! {
                () = cancellation.cancelled() => break,
                change = changes.next() => {
                    if change.is_none() { break; }
                    state.subscriptions.notify_resources_changed().await;
                }
            }
        }
    })
}

pub(super) async fn update_task(
    state: &AppState,
    task_id: TaskId,
    transition: TaskTransition,
) -> Result<TaskSnapshot, TaskError> {
    let transition = if state.tasks.is_cancel_requested(task_id).await? {
        TaskTransition::Cancelled
    } else {
        transition
    };
    state.tasks.transition(task_id, transition).await
}
