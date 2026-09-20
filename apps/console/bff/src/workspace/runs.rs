//! Fixed-profile projections for admitted agents and durable run control.
use super::{Parameters, forward};
use crate::AppState;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, Method},
    response::Response,
    routing::{delete, get, post},
};
use uuid::Uuid;
use veoveo_mcp_contract::workspace as wire;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/workspace/api/agents", get(catalog))
        .route("/workspace/api/chats/{chat}/activity", get(activity))
        .route("/workspace/api/chats/{chat}/agents", post(add))
        .route("/workspace/api/chats/{chat}/agents/{agent}", delete(remove))
        .route(
            "/workspace/api/chats/{chat}/agents/{agent}/revision",
            get(preview_revision).post(adopt_revision),
        )
        .route("/workspace/api/chats/{chat}/runs", post(start))
        .route(
            "/workspace/api/chats/{chat}/runs/{run}/cancel",
            post(cancel),
        )
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CatalogPage {
    pub after: Option<veoveo_mcp_contract::agent_management::AgentDefinitionId>,
}
async fn catalog(
    State(state): State<AppState>,
    Query(page): Query<CatalogPage>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::AgentCatalogPage>(
        &state,
        &headers,
        Method::GET,
        "/agents",
        Parameters::AgentCatalog(page),
        None,
    )
    .await
}
async fn activity(
    State(state): State<AppState>,
    Path(chat): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::AgentActivity>(
        &state,
        &headers,
        Method::GET,
        &format!("/chats/{chat}/activity"),
        Parameters::None,
        None,
    )
    .await
}
async fn add(
    State(state): State<AppState>,
    Path(chat): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<wire::AddAgent>,
) -> Response {
    forward::<_, wire::ChatAgent>(
        &state,
        &headers,
        Method::POST,
        &format!("/chats/{chat}/agents"),
        Parameters::None,
        Some(&request),
    )
    .await
}
async fn remove(
    State(state): State<AppState>,
    Path((chat, agent)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::ChatAgent>(
        &state,
        &headers,
        Method::DELETE,
        &format!("/chats/{chat}/agents/{agent}"),
        Parameters::None,
        None,
    )
    .await
}
async fn start(
    State(state): State<AppState>,
    Path(chat): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<wire::StartRun>,
) -> Response {
    forward::<_, wire::Run>(
        &state,
        &headers,
        Method::POST,
        &format!("/chats/{chat}/runs"),
        Parameters::None,
        Some(&request),
    )
    .await
}
async fn cancel(
    State(state): State<AppState>,
    Path((chat, run)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::Run>(
        &state,
        &headers,
        Method::POST,
        &format!("/chats/{chat}/runs/{run}/cancel"),
        Parameters::None,
        None,
    )
    .await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionTarget {
    revision: Option<veoveo_mcp_contract::Sha256Digest>,
}
async fn preview_revision(
    State(state): State<AppState>,
    Path((chat, agent)): Path<(Uuid, Uuid)>,
    Query(target): Query<RevisionTarget>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::AgentRevisionPreview>(
        &state,
        &headers,
        Method::GET,
        &format!("/chats/{chat}/agents/{agent}/revision"),
        Parameters::AgentRevision(target.revision),
        None,
    )
    .await
}
async fn adopt_revision(
    State(state): State<AppState>,
    Path((chat, agent)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    Json(request): Json<wire::UpdateChatAgent>,
) -> Response {
    forward::<_, wire::ChatAgent>(
        &state,
        &headers,
        Method::POST,
        &format!("/chats/{chat}/agents/{agent}/revision"),
        Parameters::None,
        Some(&request),
    )
    .await
}
