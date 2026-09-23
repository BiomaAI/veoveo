//! Fixed Workspace profile, cookie authentication and the enclosing CSRF guard.
use crate::{AppState, api};
use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{MatchedPath, Path, Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use std::time::Duration;
use uuid::Uuid;
use veoveo_speech_contract::dictation::{DictationSnapshot, MAX_CHUNK_BYTES, StartDictation};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/workspace/api/speech/dictation", post(proxy))
        .route(
            "/workspace/api/speech/dictation/{id}",
            get(proxy).delete(proxy),
        )
        .route(
            "/workspace/api/speech/dictation/{id}/chunks/{sequence}",
            put(proxy),
        )
        .route("/workspace/api/speech/dictation/{id}/finish", post(proxy))
}
#[derive(Deserialize)]
struct Route {
    id: Option<Uuid>,
    sequence: Option<u32>,
}

async fn proxy(
    State(state): State<AppState>,
    Path(route): Path<Route>,
    matched: MatchedPath,
    request: Request,
) -> Response {
    if request.uri().query().is_some() || route.id.is_some_and(|id| id.is_nil()) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let session = match api::upstream_session(&state, request.headers()).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let settled = match api::response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let method = request.method().clone();
    let path = match (route.id, route.sequence) {
        (None, None) => "/dictation".into(),
        (Some(id), Some(sequence)) => format!("/dictation/{id}/chunks/{sequence}"),
        (Some(id), None) if matched.as_str().ends_with("/finish") => {
            format!("/dictation/{id}/finish")
        }
        (Some(id), None) => format!("/dictation/{id}"),
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let result = tokio::time::timeout(Duration::from_secs(28), async {
        let body = to_bytes(request.into_body(), MAX_CHUNK_BYTES)
            .await
            .map_err(|_| StatusCode::PAYLOAD_TOO_LARGE)?;
        let expected = if let Some(id) = route.id {
            id
        } else {
            let input: StartDictation =
                serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
            input.id
        };
        let response = state
            .http
            .request(method, state.config.speech_url(&path))
            .header(header::HOST, state.config.gateway_host())
            .header(
                header::CONTENT_TYPE,
                if route.sequence.is_some() {
                    "application/octet-stream"
                } else {
                    "application/json"
                },
            )
            .bearer_auth(&session.session.access_token)
            .body(body)
            .send()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        if response.status() != StatusCode::OK {
            return Err(if response.status().is_client_error() {
                response.status()
            } else {
                StatusCode::BAD_GATEWAY
            });
        }
        let bytes = to_bytes(Body::from_stream(response.bytes_stream()), 4 * 1024 * 1024)
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        let snapshot: DictationSnapshot =
            serde_json::from_slice(&bytes).map_err(|_| StatusCode::BAD_GATEWAY)?;
        if snapshot.id != expected {
            return Err(StatusCode::BAD_GATEWAY);
        }
        Ok(Json(snapshot).into_response())
    })
    .await;
    let mut response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(StatusCode::UNAUTHORIZED)) => return api::unauthorized(&state),
        Ok(Err(status)) => status.into_response(),
        Err(_) => StatusCode::GATEWAY_TIMEOUT.into_response(),
    };
    response.headers_mut().extend(settled);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store".parse().expect("static header"),
    );
    response
}
