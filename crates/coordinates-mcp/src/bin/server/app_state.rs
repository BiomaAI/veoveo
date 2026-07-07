use rmcp::{RoleServer, model::TaskStatus, service::Peer};
use serde_json::Value;
use tokio::sync::RwLock;
use veoveo_coordinates_mcp::{artifacts::ArtifactRepository, state::CoordinatesState};
use veoveo_mcp_contract::{GatewayInternalTokenVerifier, TaskStore, notify_task_status};

use super::ownership::TaskOwnerMap;

pub(super) struct AppState {
    pub(super) tasks: TaskStore,
    pub(super) coordinates: CoordinatesState,
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
    if let Some(snapshot) = state
        .tasks
        .update(task_id, status, message, payload, error)
        .await
    {
        notify_task_status(peer, snapshot).await;
    }
}
