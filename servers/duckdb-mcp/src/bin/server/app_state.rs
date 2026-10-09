use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::sync::Mutex;
use veoveo_duckdb_mcp::{artifacts::ArtifactRepository, engine::EngineSettings};
use veoveo_duckdb_runtime::HttpsSourcePolicy;
use veoveo_task_runtime::{TaskRuntime, TaskTransition};
use veoveo_types::TaskId;

#[derive(Debug, Clone)]
pub(super) struct Caps {
    pub(super) max_inline_rows: u64,
    pub(super) max_inline_bytes: u64,
    pub(super) default_timeout_ms: u64,
    pub(super) max_timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub(super) struct ServerDirs {
    pub(super) database_dir: PathBuf,
    pub(super) exchange_dir: PathBuf,
}

pub(super) struct AppState {
    pub(super) tasks: TaskRuntime,
    pub(super) artifacts: ArtifactRepository,
    pub(super) engine: EngineSettings,
    pub(super) dirs: ServerDirs,
    pub(super) caps: Caps,
    pub(super) source_policy: HttpsSourcePolicy,
    pub(super) max_artifact_bytes: u64,
    /// One writer at a time per database file; readers go around this.
    write_locks: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        tasks: TaskRuntime,
        artifacts: ArtifactRepository,
        engine: EngineSettings,
        dirs: ServerDirs,
        caps: Caps,
        source_policy: HttpsSourcePolicy,
        max_artifact_bytes: u64,
    ) -> Self {
        Self {
            tasks,
            artifacts,
            engine,
            dirs,
            caps,
            source_policy,
            max_artifact_bytes,
            write_locks: Mutex::new(HashMap::new()),
        }
    }

    pub(super) async fn write_lock(&self, file_path: &Path) -> Arc<Mutex<()>> {
        let mut locks = self.write_locks.lock().await;
        locks
            .entry(file_path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub(super) fn clamp_timeout_ms(&self, requested: Option<std::num::NonZeroU64>) -> u64 {
        requested
            .map(std::num::NonZeroU64::get)
            .unwrap_or(self.caps.default_timeout_ms)
            .clamp(1, self.caps.max_timeout_ms)
    }
}

pub(super) async fn update_task(
    state: &AppState,
    task_id: TaskId,
    transition: TaskTransition,
) -> bool {
    update_task_with_stop(state, task_id, transition, None).await
}

pub(super) async fn update_task_with_stop(
    state: &AppState,
    task_id: TaskId,
    transition: TaskTransition,
    stop: Option<&tokio_util::sync::CancellationToken>,
) -> bool {
    let current = match state.tasks.get(task_id).await {
        Ok(Some(current)) => current,
        _ => {
            tracing::warn!(%task_id, "failed to read durable task for transition");
            return false;
        }
    };
    let result = if current.recovery_class == veoveo_task_runtime::RecoveryClass::Resume {
        state
            .tasks
            .transition_resumable_if_current(
                &current,
                transition,
                veoveo_task_runtime::ResumeCancellationPolicy::CancellationWins,
                stop,
            )
            .await
    } else {
        settle_interrupted(&state.tasks, &current, transition).await
    };
    match result {
        Ok(snapshot) => snapshot.status == veoveo_task_runtime::TaskStatus::Running,
        Err(error) => {
            tracing::warn!(%task_id, "failed to transition durable task: {error}");
            false
        }
    }
}

/// DuckDB mutation settlement changes Task delivery, never the committed database effect.
pub(super) async fn settle_interrupted(
    runtime: &TaskRuntime,
    admitted: &veoveo_task_runtime::TaskSnapshot,
    transition: TaskTransition,
) -> Result<veoveo_task_runtime::TaskSnapshot, veoveo_task_runtime::TaskError> {
    use veoveo_task_runtime::{TaskError, TaskStatus};
    if matches!(transition, TaskTransition::CancelRequested) {
        return Err(TaskError::InvalidRecord(
            "mutation execution cannot request cancellation".into(),
        ));
    }
    let current = runtime
        .get(admitted.task_id)
        .await?
        .ok_or_else(|| TaskError::NotFound(admitted.task_id.to_string()))?;
    mutation_identity(runtime, admitted, &current)?;
    if current.is_terminal() {
        return Ok(current);
    }
    mutation_lease(runtime, &current)?;
    let (selected, transition) = if current.status == TaskStatus::CancelRequested {
        (&current, TaskTransition::Cancelled)
    } else {
        (admitted, transition)
    };
    match runtime.transition_if_current(selected, transition).await {
        Ok(settled) => Ok(settled),
        Err(error @ (TaskError::Conflict(_) | TaskError::InvalidTransition { .. })) => {
            let observed = runtime
                .get(admitted.task_id)
                .await?
                .ok_or_else(|| TaskError::NotFound(admitted.task_id.to_string()))?;
            mutation_identity(runtime, admitted, &observed)?;
            if observed.is_terminal() {
                return Ok(observed);
            }
            mutation_lease(runtime, &observed)?;
            if observed.status != TaskStatus::CancelRequested {
                return Err(error);
            }
            runtime
                .transition_if_current(&observed, TaskTransition::Cancelled)
                .await
        }
        Err(error) => Err(error),
    }
}

fn mutation_identity(
    runtime: &TaskRuntime,
    admitted: &veoveo_task_runtime::TaskSnapshot,
    current: &veoveo_task_runtime::TaskSnapshot,
) -> Result<(), veoveo_task_runtime::TaskError> {
    use veoveo_task_runtime::{RecoveryClass, TaskError};
    if current.server != runtime.server() {
        return Err(TaskError::WrongServer(current.task_id.to_string()));
    }
    if admitted.recovery_class != RecoveryClass::InterruptedIndeterminate
        || current.recovery_class != RecoveryClass::InterruptedIndeterminate
    {
        return Err(TaskError::InvalidRecord(
            "DuckDB mutation settlement requires InterruptedIndeterminate".into(),
        ));
    }
    if admitted.task_id != current.task_id
        || admitted.server != current.server
        || admitted.task_type != current.task_type
        || admitted.owner != current.owner
        || admitted.request != current.request
        || admitted.created_at != current.created_at
        || admitted.ttl_ms != current.ttl_ms
        || admitted.poll_interval_ms != current.poll_interval_ms
        || admitted.idempotency_key != current.idempotency_key
    {
        return Err(TaskError::Conflict(admitted.task_id.to_string()));
    }
    Ok(())
}

fn mutation_lease(
    runtime: &TaskRuntime,
    current: &veoveo_task_runtime::TaskSnapshot,
) -> Result<(), veoveo_task_runtime::TaskError> {
    if current.lease_owner.as_deref() != Some(runtime.worker_id())
        || current
            .lease_expires_at
            .is_none_or(|expiry| expiry <= chrono::Utc::now())
    {
        return Err(veoveo_task_runtime::TaskError::LeaseHeld(
            current.task_id.to_string(),
        ));
    }
    Ok(())
}
