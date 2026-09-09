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
    let is_part = method == Method::PUT && route.part_number.is_some();
    // Ingress can delay a part's headers while receiving its body. Rotating an
    // old cookie here can replay a refresh token consumed by a concurrent short
    // request. Parts use their current access token; short control reads refresh.
    let session = match if is_part {
        part_session(&state, &request_headers)
    } else {
        api::upstream_session(&state, &request_headers).await
    } {
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
        return if is_part {
            part_unauthorized()
        } else {
            api::unauthorized(&state)
        };
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

fn part_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<crate::oauth::UpstreamSession, Response> {
    let session =
        crate::session::read_session(headers, &state.sessions).ok_or_else(part_unauthorized)?;
    validate_part_session(
        session,
        state.config.oauth_scopes(),
        chrono::Utc::now().timestamp(),
    )
}

fn validate_part_session(
    session: crate::session::ConsoleSession,
    scopes: &std::collections::BTreeSet<veoveo_mcp_contract::ScopeName>,
    now: i64,
) -> Result<crate::oauth::UpstreamSession, Response> {
    if session.is_expired(now)
        || session.access_expires_at <= now
        || !scopes.is_subset(&session.granted_scopes)
    {
        return Err(part_unauthorized());
    }
    Ok(crate::oauth::UpstreamSession {
        session,
        replacement_cookie: None,
    })
}

fn part_unauthorized() -> Response {
    // A delayed request must not clear a newer cookie installed in the browser.
    (
        StatusCode::UNAUTHORIZED,
        [(header::CACHE_CONTROL, "no-store")],
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::ConsoleSession;
    use std::collections::BTreeSet;
    fn session(expires: i64) -> ConsoleSession {
        ConsoleSession {
            access_token: "access".into(),
            access_expires_at: expires,
            refresh_token: "refresh".into(),
            refresh_expires_at: 1000,
            granted_scopes: BTreeSet::new(),
            csrf_token: "csrf".into(),
        }
    }

    #[test]
    fn delayed_parts_do_not_rotate_an_access_token_in_the_refresh_window() {
        let current = session(110);
        assert!(current.should_refresh(100));
        let result = validate_part_session(current, &BTreeSet::new(), 100).unwrap();
        assert_eq!(result.session.access_token, "access");
        assert_eq!(result.session.refresh_token, "refresh");
        assert!(result.replacement_cookie.is_none());
    }

    #[test]
    fn expired_parts_cannot_clear_a_newer_browser_cookie() {
        let response = validate_part_session(session(100), &BTreeSet::new(), 100)
            .err()
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(!response.headers().contains_key(header::SET_COOKIE));
        let scopes =
            BTreeSet::from([veoveo_mcp_contract::ScopeName::new("artifact:upload").unwrap()]);
        assert!(validate_part_session(session(200), &scopes, 100).is_err());
    }
}
