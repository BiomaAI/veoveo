use super::{Parameters, forward};
use crate::AppState;
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use uuid::Uuid;
use veoveo_mcp_contract::workspace as wire;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/workspace/api/capabilities", get(capabilities))
        .route("/workspace/api/operations", get(list))
        .route("/workspace/api/operations/events", get(events))
        .route("/workspace/api/operations/{id}", get(detail))
        .route("/workspace/api/chats/{chat}/operations", post(start))
        .route("/workspace/api/operations/{id}/cancel", post(cancel))
        .route("/workspace/api/operations/{id}/input", post(answer))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActivityQuery {
    pub chat: Option<Uuid>,
    pub before: Option<Uuid>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Watch {
    ids: String,
}

async fn capabilities(State(state): State<AppState>, headers: HeaderMap) -> Response {
    forward::<(), Vec<wire::Capability>>(
        &state,
        &headers,
        Method::GET,
        "/capabilities",
        Parameters::None,
        None,
    )
    .await
}
async fn list(
    State(state): State<AppState>,
    Query(query): Query<ActivityQuery>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::OperationPage>(
        &state,
        &headers,
        Method::GET,
        "/operations",
        Parameters::Activity(query),
        None,
    )
    .await
}
async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::OperationView>(
        &state,
        &headers,
        Method::GET,
        &format!("/operations/{id}"),
        Parameters::None,
        None,
    )
    .await
}
async fn start(
    State(state): State<AppState>,
    Path(chat): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<wire::StartOperation>,
) -> Response {
    forward::<_, wire::OperationSummary>(
        &state,
        &headers,
        Method::POST,
        &format!("/chats/{chat}/operations"),
        Parameters::None,
        Some(&request),
    )
    .await
}
async fn cancel(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Response {
    forward::<(), ()>(
        &state,
        &headers,
        Method::POST,
        &format!("/operations/{id}/cancel"),
        Parameters::None,
        None,
    )
    .await
}
async fn answer(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<wire::AnswerOperation>,
) -> Response {
    forward::<_, ()>(
        &state,
        &headers,
        Method::POST,
        &format!("/operations/{id}/input"),
        Parameters::None,
        Some(&request),
    )
    .await
}
async fn events(
    State(state): State<AppState>,
    Query(query): Query<Watch>,
    headers: HeaderMap,
) -> Response {
    if query.ids.len() > 32 * 37 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(ids) = query
        .ids
        .split(',')
        .map(Uuid::parse_str)
        .collect::<Result<Vec<_>, _>>()
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    if ids.is_empty() || ids.len() > 32 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let ids = ids
        .iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>()
        .join(",");
    super::events::forward(&state, &headers, &format!("/operations/events?ids={ids}")).await
}
