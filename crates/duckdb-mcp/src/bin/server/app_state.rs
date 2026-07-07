use std::{collections::HashMap, path::PathBuf, sync::Arc};

use rmcp::{RoleServer, model::TaskStatus, service::Peer};
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};
use veoveo_duckdb_mcp::{
    artifacts::ArtifactRepository, engine::EngineSettings, state::DuckdbState,
};
use veoveo_mcp_contract::{GatewayInternalTokenVerifier, TaskStore, notify_task_status};

use super::ownership::TaskOwnerMap;

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
    pub(super) tasks: TaskStore,
    pub(super) durable: DuckdbState,
    pub(super) artifacts: ArtifactRepository,
    pub(super) internal_token_verifier: GatewayInternalTokenVerifier,
    pub(super) task_owners: RwLock<TaskOwnerMap>,
    pub(super) engine: EngineSettings,
    pub(super) dirs: ServerDirs,
    pub(super) caps: Caps,
    pub(super) ingest_allowlist: Vec<String>,
    pub(super) http: reqwest::Client,
    /// One writer at a time per database file; readers go around this.
    write_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        tasks: TaskStore,
        durable: DuckdbState,
        artifacts: ArtifactRepository,
        internal_token_verifier: GatewayInternalTokenVerifier,
        task_owners: TaskOwnerMap,
        engine: EngineSettings,
        dirs: ServerDirs,
        caps: Caps,
        ingest_allowlist: Vec<String>,
        http: reqwest::Client,
    ) -> Self {
        Self {
            tasks,
            durable,
            artifacts,
            internal_token_verifier,
            task_owners: RwLock::new(task_owners),
            engine,
            dirs,
            caps,
            ingest_allowlist,
            http,
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

pub(super) async fn update_task(
    state: &AppState,
    peer: &Peer<RoleServer>,
    task_id: &str,
    status: TaskStatus,
    message: impl Into<String>,
    payload: Option<Value>,
    error: Option<String>,
) {
    let payload_for_store = payload.clone();
    let error_for_store = error.clone();
    if let Some(snapshot) = state
        .tasks
        .update(task_id, status, message, payload, error)
        .await
    {
        if let Err(err) = state.durable.record_task(
            &snapshot,
            payload_for_store.as_ref(),
            error_for_store.as_deref(),
        ) {
            tracing::warn!(task_id, "failed to persist task update: {err}");
        }
        notify_task_status(peer, snapshot).await;
    }
}
