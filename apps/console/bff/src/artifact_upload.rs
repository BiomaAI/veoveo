//! Same-origin upload transport; the Console session selects profile and actor.

use crate::{AppState, api};
use axum::{
    Router,
    body::Body,
    extract::{MatchedPath, OriginalUri, Path, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use std::num::NonZeroU32;
use veoveo_mcp_contract::{
    ArtifactUploadId, UPLOAD_PART_BYTE_LEN_HEADER, UPLOAD_PART_SHA256_HEADER,
};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/console/api/artifact-uploads/policy", get(proxy))
        .route("/console/api/artifact-uploads", post(proxy))
        .route(
            "/console/api/artifact-uploads/{upload_id}",
            get(proxy).delete(proxy),
        )
        .route(
            "/console/api/artifact-uploads/{upload_id}/parts/{part_number}",
            put(proxy),
        )
        .route(
            "/console/api/artifact-uploads/{upload_id}/complete",
            post(proxy),
        )
}

#[derive(Default, Deserialize)]
struct Route {
    upload_id: Option<ArtifactUploadId>,
    part_number: Option<NonZeroU32>,
}

async fn proxy(
    State(state): State<AppState>,
    route: Result<Path<Route>, axum::extract::rejection::PathRejection>,
    matched: MatchedPath,
    OriginalUri(uri): OriginalUri,
    method: Method,
    request_headers: HeaderMap,
    body: Body,
) -> Response {
    let Path(route) = match route {
        Ok(route) => route,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let session = match api::upstream_session(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let mut response_headers = match api::response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let path = if matched.as_str().ends_with("/policy") {
        "upload-policy".into()
    } else if let Some(id) = route.upload_id {
        if let Some(number) = route.part_number {
            format!("uploads/{id}/parts/{number}")
        } else if matched.as_str().ends_with("/complete") {
            format!("uploads/{id}/complete")
        } else {
            format!("uploads/{id}")
        }
    } else {
        "uploads".into()
    };
    let mut url = state.config.artifact_upload_url(&path);
    if method == Method::GET {
        url.set_query(uri.query());
    }
    let mut request = state
        .stream_http
        .request(method, url)
        .header(header::HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token)
        .body(reqwest::Body::wrap_stream(body.into_data_stream()));
    for name in [
        "idempotency-key",
        UPLOAD_PART_BYTE_LEN_HEADER,
        UPLOAD_PART_SHA256_HEADER,
        "content-type",
        "content-length",
    ] {
        for value in request_headers.get_all(name) {
            request = request.header(name, value);
        }
    }
    let upstream = match request.send().await {
        Ok(upstream) => upstream,
        Err(error) => {
            tracing::warn!(error = %error.without_url(), "console upload request interrupted");
            return (StatusCode::SERVICE_UNAVAILABLE, response_headers).into_response();
        }
    };
    if upstream.status() == StatusCode::UNAUTHORIZED {
        return api::unauthorized(&state);
    }
    if upstream.status().is_redirection() {
        return (StatusCode::BAD_GATEWAY, response_headers).into_response();
    }
    let status = upstream.status();
    for name in [
        header::CONTENT_TYPE,
        header::CONTENT_LENGTH,
        header::RETRY_AFTER,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            response_headers.insert(name, value.clone());
        }
    }
    (
        status,
        response_headers,
        Body::from_stream(upstream.bytes_stream()),
    )
        .into_response()
}
