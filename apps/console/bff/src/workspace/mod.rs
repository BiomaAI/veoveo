//! Shared browser edge for the independent Workspace client. The upstream origin
//! and profile are installation configuration; credentials come only from cookies.
#[cfg(test)]
mod tests;
use std::time::Duration;

use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, Method, StatusCode, header::HOST},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;
use veoveo_mcp_contract::workspace as wire;

use crate::{AppState, api};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/workspace/api/session", get(session))
        .route("/workspace/api/chats", get(chats).post(create))
        .route("/workspace/api/chats/{chat}", get(snapshot).put(settings))
        .route("/workspace/api/chats/{chat}/messages", post(send))
        .route(
            "/workspace/api/chats/{chat}/members/{person}",
            delete(remove),
        )
        .route("/workspace/api/chats/{chat}/invitations", post(invite))
        .route("/workspace/api/invitations", get(invitations))
        .route("/workspace/api/invitations/{invitation}", post(decide))
        .route("/workspace/api/people", get(people))
        .layer(DefaultBodyLimit::max(64 * 1024))
}

enum Parameters {
    None,
    Page(i64),
    Search(String),
}

async fn forward<T: Serialize, R: DeserializeOwned + Serialize>(
    state: &AppState,
    headers: &HeaderMap,
    method: Method,
    path: &str,
    parameters: Parameters,
    body: Option<&T>,
) -> Response {
    let session = match api::upstream_session(state, headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let settled_headers = match api::response_session_headers(state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let mut url = state.config.workspace_url(path);
        match parameters {
            Parameters::None => {}
            Parameters::Page(after) => {
                url.query_pairs_mut()
                    .append_pair("after", &after.to_string());
            }
            Parameters::Search(query) => {
                url.query_pairs_mut().append_pair("q", &query);
            }
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
        if status == StatusCode::NO_CONTENT {
            return Ok(status.into_response());
        }
        if status != StatusCode::OK {
            return Err(if status.is_client_error() || status.is_server_error() {
                status
            } else {
                StatusCode::BAD_GATEWAY
            });
        }
        // A page contains at most 100 messages of 32 KiB plus member metadata.
        let bytes = to_bytes(Body::from_stream(upstream.bytes_stream()), 4 * 1024 * 1024)
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        let value: R = serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_GATEWAY)?;
        Ok(Json(value).into_response())
    })
    .await;
    let mut response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(StatusCode::UNAUTHORIZED)) => return api::unauthorized(state),
        Ok(Err(status)) => status.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    response.headers_mut().extend(settled_headers);
    response
}

async fn session(State(state): State<AppState>, headers: HeaderMap) -> Response {
    forward::<(), wire::WorkspaceBootstrap>(
        &state,
        &headers,
        Method::GET,
        "/session",
        Parameters::None,
        None,
    )
    .await
}

async fn chats(State(state): State<AppState>, headers: HeaderMap) -> Response {
    forward::<(), Vec<wire::Chat>>(
        &state,
        &headers,
        Method::GET,
        "/chats",
        Parameters::None,
        None,
    )
    .await
}

async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<wire::CreateChat>,
) -> Response {
    forward::<_, wire::Chat>(
        &state,
        &headers,
        Method::POST,
        "/chats",
        Parameters::None,
        Some(&request),
    )
    .await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default)]
    after: i64,
}

async fn snapshot(
    State(state): State<AppState>,
    Path(chat): Path<Uuid>,
    Query(page): Query<Page>,
    headers: HeaderMap,
) -> Response {
    forward::<(), wire::ChatSnapshot>(
        &state,
        &headers,
        Method::GET,
        &format!("/chats/{chat}"),
        Parameters::Page(page.after),
        None,
    )
    .await
}

async fn send(
    State(state): State<AppState>,
    Path(chat): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<wire::SendMessage>,
) -> Response {
    forward::<_, wire::Message>(
        &state,
        &headers,
        Method::POST,
        &format!("/chats/{chat}/messages"),
        Parameters::None,
        Some(&request),
    )
    .await
}

async fn invite(
    State(state): State<AppState>,
    Path(chat): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<wire::InvitePerson>,
) -> Response {
    forward::<_, wire::Invitation>(
        &state,
        &headers,
        Method::POST,
        &format!("/chats/{chat}/invitations"),
        Parameters::None,
        Some(&request),
    )
    .await
}

async fn invitations(State(state): State<AppState>, headers: HeaderMap) -> Response {
    forward::<(), Vec<wire::Invitation>>(
        &state,
        &headers,
        Method::GET,
        "/invitations",
        Parameters::None,
        None,
    )
    .await
}

async fn decide(
    State(state): State<AppState>,
    Path(invitation): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<wire::DecideInvitation>,
) -> Response {
    forward::<_, wire::Invitation>(
        &state,
        &headers,
        Method::POST,
        &format!("/invitations/{invitation}"),
        Parameters::None,
        Some(&request),
    )
    .await
}

async fn remove(
    State(state): State<AppState>,
    Path((chat, person)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
) -> Response {
    forward::<(), ()>(
        &state,
        &headers,
        Method::DELETE,
        &format!("/chats/{chat}/members/{person}"),
        Parameters::None,
        None,
    )
    .await
}

async fn settings(
    State(state): State<AppState>,
    Path(chat): Path<Uuid>,
    headers: HeaderMap,
    Json(request): Json<wire::ChatSettings>,
) -> Response {
    forward::<_, wire::Chat>(
        &state,
        &headers,
        Method::PUT,
        &format!("/chats/{chat}"),
        Parameters::None,
        Some(&request),
    )
    .await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    q: String,
}

async fn people(
    State(state): State<AppState>,
    Query(search): Query<Search>,
    headers: HeaderMap,
) -> Response {
    if search.q.len() > 128 || search.q.trim().chars().count() < 2 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    forward::<(), Vec<wire::Person>>(
        &state,
        &headers,
        Method::GET,
        "/people",
        Parameters::Search(search.q),
        None,
    )
    .await
}
