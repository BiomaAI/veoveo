//! Fixed-profile projections for admitted agents and durable run control.
use super::{Parameters, forward};
use crate::AppState;
use axum::{
    Json, Router,
    extract::{Path, State},
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
        .route("/workspace/api/chats/{chat}/runs", post(start))
        .route(
            "/workspace/api/chats/{chat}/runs/{run}/cancel",
            post(cancel),
        )
}
async fn catalog(State(state): State<AppState>, headers: HeaderMap) -> Response {
    forward::<(), Vec<wire::AgentDefinition>>(
        &state,
        &headers,
        Method::GET,
        "/agents",
        Parameters::None,
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
