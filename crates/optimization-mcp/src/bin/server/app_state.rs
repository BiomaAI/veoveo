use rmcp::{RoleServer, model::TaskStatus, service::Peer};
use serde_json::Value;
use tokio::sync::RwLock;
use veoveo_mcp_contract::{GatewayInternalTokenVerifier, TaskStore, notify_task_status};
use veoveo_optimization_mcp::{artifacts::ArtifactRepository, state::DuckdbState};

use super::ownership::TaskOwnerMap;

pub(super) struct AppState {
    pub(super) tasks: TaskStore,
    pub(super) durable: DuckdbState,
    pub(super) artifacts: ArtifactRepository,
    pub(super) internal_token_verifier: GatewayInternalTokenVerifier,
    pub(super) task_owners: RwLock<TaskOwnerMap>,
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
