use std::time::Duration;

use crate::{AppState, api};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{
        HeaderMap, StatusCode,
        header::{CONTENT_TYPE, HOST},
    },
    response::{IntoResponse, Response},
};
use futures::StreamExt;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EmptyQuery {}

pub(super) async fn events(
    State(state): State<AppState>,
    Path(chat): Path<Uuid>,
    Query(_): Query<EmptyQuery>,
    headers: HeaderMap,
) -> Response {
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
            .get(state.config.workspace_url(&format!("/chats/{chat}/events")))
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
            if upstream.status().is_client_error() || upstream.status().is_server_error() =>
        {
            upstream.status().into_response()
        }
        Ok(Ok(upstream))
            if upstream.status() == StatusCode::OK
                && upstream
                    .headers()
                    .get(CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|value| value.split(';').next() == Some("text/event-stream")) =>
        {
            // Bounded chunks and streaming backpressure; no transcript buffering.
            // The gateway owns current family, token and membership validation.
            let stream = upstream.bytes_stream().map(|chunk| match chunk {
                Ok(bytes) if bytes.len() <= 64 * 1024 => Ok(bytes),
                _ => Err(std::io::Error::other("Workspace event stream ended")),
            });
            (
                [
                    ("content-type", "text/event-stream"),
                    ("x-accel-buffering", "no"),
                ],
                Body::from_stream(stream),
            )
                .into_response()
        }
        _ => StatusCode::BAD_GATEWAY.into_response(),
    };
    response.headers_mut().extend(settled);
    response
}
