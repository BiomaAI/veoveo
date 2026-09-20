//! Both clients use fixed-profile agent routes and their own cookie/CSRF authority.
use std::time::Duration;

use crate::{AppState, api, browser::BrowserApp};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{
        HeaderMap, Method, StatusCode,
        header::{CONTENT_TYPE, HOST},
    },
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures::StreamExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use veoveo_mcp_contract::agent_management as wire;

pub(crate) fn router(app: BrowserApp) -> Router<AppState> {
    Router::new().nest(
        app.api_root(),
        Router::new()
            .route("/agent-authoring", get(authoring))
            .route("/agent-models", get(models))
            .route("/agent-templates", get(templates))
            .route("/agent-capabilities", get(capabilities))
            .route("/agent-events", get(events))
            .route("/agent-definitions", get(list).post(create))
            .route("/agent-definitions/{id}", get(read).patch(metadata))
            .route("/agent-definitions/{id}/draft", get(draft).put(save))
            .route("/agent-definitions/{id}/revisions", get(revisions))
            .route("/agent-definitions/{id}/validate", post(validate))
            .route("/agent-definitions/{id}/publish", post(publish))
            .route("/agent-definitions/{id}/disable", post(disable))
            .route("/agent-definitions/{id}/enable", post(enable))
            .route("/agent-definitions/{id}/archive", post(archive))
            .layer(DefaultBodyLimit::max(80 * 1024)),
    )
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(skip_serializing_if = "Option::is_none")]
    after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

async fn forward<T: Serialize, R: DeserializeOwned + Serialize>(
    state: &AppState,
    headers: &HeaderMap,
    method: Method,
    path: &str,
    page: Page,
    body: Option<&T>,
) -> Response {
    if page.after.as_ref().is_some_and(|s| s.len() > 128)
        || page.limit.is_some_and(|n| !(1..=200).contains(&n))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let session = match api::upstream_session(state, headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let settled = match api::response_session_headers(state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let mut url = state.config.admin_url(path);
        if let Some(after) = &page.after {
            url.query_pairs_mut().append_pair("after", after);
        }
        if let Some(limit) = page.limit {
            url.query_pairs_mut()
                .append_pair("limit", &limit.to_string());
        }
        let mut request = state
            .http
            .request(method, url)
            .header(HOST, state.config.gateway_host())
            .bearer_auth(&session.session.access_token);
        if let Some(body) = body {
            request = request.json(body);
        }
        let upstream = request.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
        let status = upstream.status();
        if status != StatusCode::OK && status != StatusCode::UNPROCESSABLE_ENTITY {
            return Err(if status.is_client_error() || status.is_server_error() {
                status
            } else {
                StatusCode::BAD_GATEWAY
            });
        }
        let bytes = to_bytes(Body::from_stream(upstream.bytes_stream()), 4 * 1024 * 1024)
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        if status == StatusCode::UNPROCESSABLE_ENTITY {
            let value: wire::Validation =
                serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_GATEWAY)?;
            Ok((status, Json(value)).into_response())
        } else {
            let value: R = serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_GATEWAY)?;
            Ok(Json(value).into_response())
        }
    })
    .await;
    let mut response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(StatusCode::UNAUTHORIZED)) => return api::unauthorized(state),
        Ok(Err(status)) => status.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    response.headers_mut().extend(settled);
    response
}

macro_rules! read_root {
    ($name:ident, $path:literal, $output:ty) => {
        async fn $name(State(state): State<AppState>, headers: HeaderMap) -> Response {
            forward::<(), $output>(&state, &headers, Method::GET, $path, Page::default(), None)
                .await
        }
    };
}
read_root!(authoring, "agent-authoring", wire::Authoring);
read_root!(models, "agent-models", Vec<wire::ModelChoice>);
read_root!(templates, "agent-templates", Vec<wire::TemplateChoice>);
read_root!(
    capabilities,
    "agent-capabilities",
    Vec<wire::CapabilityChoice>
);

async fn list(
    State(state): State<AppState>,
    Query(page): Query<Page>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::DefinitionPage>(
        &state,
        &headers,
        Method::GET,
        "agent-definitions",
        page,
        None,
    )
    .await
}
async fn revisions(
    State(state): State<AppState>,
    Path(id): Path<wire::AgentDefinitionId>,
    Query(page): Query<Page>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::RevisionPage>(
        &state,
        &headers,
        Method::GET,
        &format!("agent-definitions/{id}/revisions"),
        page,
        None,
    )
    .await
}
async fn read(
    State(state): State<AppState>,
    Path(id): Path<wire::AgentDefinitionId>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::Definition>(
        &state,
        &headers,
        Method::GET,
        &format!("agent-definitions/{id}"),
        Page::default(),
        None,
    )
    .await
}
async fn draft(
    State(state): State<AppState>,
    Path(id): Path<wire::AgentDefinitionId>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::Draft>(
        &state,
        &headers,
        Method::GET,
        &format!("agent-definitions/{id}/draft"),
        Page::default(),
        None,
    )
    .await
}
async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<wire::CreateDefinition>,
) -> Response {
    forward::<_, wire::Definition>(
        &state,
        &headers,
        Method::POST,
        "agent-definitions",
        Page::default(),
        Some(&request),
    )
    .await
}
async fn metadata(
    State(state): State<AppState>,
    Path(id): Path<wire::AgentDefinitionId>,
    headers: HeaderMap,
    Json(request): Json<wire::UpdateMetadata>,
) -> Response {
    forward::<_, wire::Definition>(
        &state,
        &headers,
        Method::PATCH,
        &format!("agent-definitions/{id}"),
        Page::default(),
        Some(&request),
    )
    .await
}
macro_rules! mutation {
    ($name:ident, $method:ident, $suffix:literal, $input:ty, $output:ty) => {
        async fn $name(
            State(state): State<AppState>,
            Path(id): Path<wire::AgentDefinitionId>,
            headers: HeaderMap,
            Json(request): Json<$input>,
        ) -> Response {
            forward::<_, $output>(
                &state,
                &headers,
                Method::$method,
                &format!("agent-definitions/{id}/{}", $suffix),
                Page::default(),
                Some(&request),
            )
            .await
        }
    };
}
mutation!(save, PUT, "draft", wire::SaveDraft, wire::Definition);
mutation!(
    validate,
    POST,
    "validate",
    wire::ValidateDefinition,
    wire::Validation
);
mutation!(
    publish,
    POST,
    "publish",
    wire::PublishDefinition,
    wire::Definition
);
mutation!(
    disable,
    POST,
    "disable",
    wire::RevisionRequest,
    wire::Definition
);
mutation!(
    enable,
    POST,
    "enable",
    wire::RevisionRequest,
    wire::Definition
);
mutation!(
    archive,
    POST,
    "archive",
    wire::RevisionRequest,
    wire::Definition
);

async fn events(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let session = match api::upstream_session(&state, &headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let settled = match api::response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let upstream = tokio::time::timeout(
        Duration::from_secs(10),
        state
            .stream_http
            .get(state.config.admin_url("agent-events"))
            .header(HOST, state.config.gateway_host())
            .bearer_auth(&session.session.access_token)
            .send(),
    )
    .await;
    let mut response = match upstream {
        Ok(Ok(upstream)) if upstream.status() == StatusCode::UNAUTHORIZED => {
            return api::unauthorized(&state);
        }
        Ok(Ok(upstream))
            if upstream.status().is_success()
                && upstream
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|h| h.to_str().ok())
                    .is_some_and(|h| h.starts_with("text/event-stream")) =>
        {
            let stream =
                futures::stream::unfold(upstream.bytes_stream(), |mut chunks| async move {
                    match tokio::time::timeout(Duration::from_secs(30), chunks.next()).await {
                        Ok(Some(Ok(bytes))) if bytes.len() <= 64 * 1024 => {
                            Some((Ok::<_, std::io::Error>(bytes), chunks))
                        }
                        _ => None,
                    }
                });
            (
                [
                    (CONTENT_TYPE, "text/event-stream"),
                    (
                        axum::http::HeaderName::from_static("x-accel-buffering"),
                        "no",
                    ),
                ],
                Body::from_stream(stream),
            )
                .into_response()
        }
        Ok(Ok(upstream)) if upstream.status().is_client_error() => {
            upstream.status().into_response()
        }
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    response.headers_mut().extend(settled);
    response
}
