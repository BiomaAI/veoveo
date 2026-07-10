use std::{collections::HashMap, path::PathBuf, sync::Arc};

use tokio::sync::Mutex;
use veoveo_duckdb_mcp::{artifacts::ArtifactRepository, engine::EngineSettings};
use veoveo_duckdb_runtime::HttpsSourcePolicy;
use veoveo_task_runtime::{TaskRuntime, TaskTransition};

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
    write_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
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

    pub(super) async fn write_lock(&self, file_path: &str) -> Arc<Mutex<()>> {
        let mut locks = self.write_locks.lock().await;
        locks
            .entry(file_path.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub(super) fn clamp_timeout_ms(&self, requested: Option<u64>) -> u64 {
        requested
            .unwrap_or(self.caps.default_timeout_ms)
            .clamp(1, self.caps.max_timeout_ms)
    }
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
        tracing::warn!(task_id, "failed to transition durable task: {error}");
    }
}
