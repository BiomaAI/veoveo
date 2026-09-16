//! Shared App hosting with Workspace's cookie authority and durable Task journal.
use super::{Parameters, forward};
use crate::{AppState, apps};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::{HeaderMap, Method},
    response::Response,
    routing::{get, post},
};
use uuid::Uuid;
use veoveo_mcp_contract::workspace as wire;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/workspace/api/apps", get(apps::list_apps))
        .route("/workspace/api/apps/events", get(apps::app_catalog_events))
        .route("/workspace/api/apps/frame", get(apps::app_frame))
        .route("/workspace/api/apps/read", post(apps::read_app_resource))
        .route(
            "/workspace/api/apps/resource-events",
            post(apps::app_resource_events),
        )
        .route(
            "/workspace/api/apps/unsubscribe",
            post(apps::unsubscribe_app_resource),
        )
        .route("/workspace/api/chats/{chat}/app-operations", post(start))
        .route("/workspace/api/app-operations/{id}", post(detail))
        .route("/workspace/api/app-tasks/get", post(get_task))
        .route("/workspace/api/app-tasks/update", post(update_task))
        .route("/workspace/api/app-tasks/cancel", post(cancel_task))
}

async fn start(
    State(state): State<AppState>,
    Path(chat): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<wire::StartAppOperation>,
) -> Response {
    forward::<_, wire::OperationSummary>(
        &state,
        &headers,
        Method::POST,
        &format!("/chats/{chat}/app-operations"),
        Parameters::None,
        Some(&request),
    )
    .await
}
async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<wire::AppOrigin>,
) -> Response {
    forward::<_, wire::AppOperationView>(
        &state,
        &headers,
        Method::POST,
        &format!("/app-operations/{id}"),
        Parameters::None,
        Some(&request),
    )
    .await
}
async fn get_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<wire::AppTaskRequest>,
) -> Response {
    forward::<_, rmcp::model::GetTaskResult>(
        &state,
        &headers,
        Method::POST,
        "/app-tasks/get",
        Parameters::None,
        Some(&request),
    )
    .await
}
async fn update_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<wire::UpdateAppTask>,
) -> Response {
    forward::<_, rmcp::model::TaskAckResult>(
        &state,
        &headers,
        Method::POST,
        "/app-tasks/update",
        Parameters::None,
        Some(&request),
    )
    .await
}
async fn cancel_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<wire::AppTaskRequest>,
) -> Response {
    forward::<_, rmcp::model::TaskAckResult>(
        &state,
        &headers,
        Method::POST,
        "/app-tasks/cancel",
        Parameters::None,
        Some(&request),
    )
    .await
}
