use veoveo_mcp_contract::{ResourceListObservers, SubscriptionHub};
use veoveo_optimization_mcp::{
    artifacts::ArtifactRepository,
    executor::{ExecutorClient, ExecutorHealth},
    problem_store::ProblemStore,
};
use veoveo_task_runtime::{TaskRuntime, TaskTransition};

pub(super) struct AppState {
    pub(super) tasks: TaskRuntime,
    pub(super) artifacts: ArtifactRepository,
    pub(super) executor: ExecutorClient,
    pub(super) executor_health: ExecutorHealth,
    pub(super) problem_store: ProblemStore,
    pub(super) subscriptions: std::sync::Arc<SubscriptionHub>,
    pub(super) resource_observers: std::sync::Arc<ResourceListObservers>,
    pub(super) max_artifact_bytes: u64,
    pub(super) max_executor_frame_bytes: u64,
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
