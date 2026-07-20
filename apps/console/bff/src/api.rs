use axum::{
    body::Body,
    extract::{Path, Query, RawQuery, State},
    http::{
        HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode,
        header::{
            ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
            CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, HOST, IF_MATCH, IF_MODIFIED_SINCE,
            IF_NONE_MATCH, IF_RANGE, IF_UNMODIFIED_SINCE, LAST_MODIFIED, LOCATION, RANGE,
            REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS,
        },
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use futures::StreamExt;
use serde::Serialize;
use veoveo_mcp_contract::{
    AccessSubject, ArtifactAccessRequestId, ArtifactAccessRequestScope, ArtifactAccessRequestState,
    ArtifactId, ArtifactShareLinkId, CreateArtifactAccessRequest, CreateArtifactShareLinkRequest,
    DecideArtifactAccessRequest, ListArtifactAccessRequests, PutGrantRequest,
    SetArtifactReleaseStateRequest,
};
use veoveo_mcp_task_extension::ProtocolTaskId;

use crate::{
    AppState,
    session::{clear_session_cookie, read_session, set_session_cookie},
};

const MAX_SNAPSHOT_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const CSRF_HEADER: &str = "x-veoveo-csrf-token";

#[derive(Debug, PartialEq, Eq)]
enum SnapshotUpstreamDisposition {
    Success,
    Unauthorized,
    Forbidden,
    BadGateway,
}

pub(crate) async fn enforce_csrf(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    if matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) {
        return next.run(request).await;
    }

    let Some(session) = read_session(request.headers(), &state.sessions) else {
        return unauthorized(&state);
    };
    if session.is_expired(Utc::now().timestamp()) {
        return unauthorized(&state);
    }
    let supplied = request
        .headers()
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok());
    if !supplied.is_some_and(|value| constant_time_equal(value, &session.csrf_token)) {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

pub(crate) async fn snapshot(
    State(state): State<AppState>,
    request_headers: HeaderMap,
) -> Response {
    let Some(session) = read_session(&request_headers, &state.sessions) else {
        return unauthorized(&state);
    };
    if session.is_expired(Utc::now().timestamp()) {
        return unauthorized(&state);
    }
    let session = match crate::oauth::upstream_session(&state, session).await {
        Ok(session) => session,
        Err(error) => {
            tracing::warn!(%error, "console session refresh failed");
            return unauthorized(&state);
        }
    };
    let mut response_headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let upstream = match state
        .http
        .get(state.config.snapshot_url())
        .header(HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "console snapshot upstream failed");
            return (response_headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    let status = upstream.status();
    match classify_snapshot_upstream(status) {
        SnapshotUpstreamDisposition::Success => {}
        SnapshotUpstreamDisposition::Unauthorized => return unauthorized(&state),
        SnapshotUpstreamDisposition::Forbidden => {
            return (response_headers, StatusCode::FORBIDDEN).into_response();
        }
        SnapshotUpstreamDisposition::BadGateway => {
            tracing::warn!(%status, "console snapshot upstream returned an error");
            return (response_headers, StatusCode::BAD_GATEWAY).into_response();
        }
    }
    if upstream
        .content_length()
        .is_some_and(|length| length > MAX_SNAPSHOT_BYTES)
    {
        return (response_headers, StatusCode::BAD_GATEWAY).into_response();
    }
    let body = match upstream.bytes().await {
        Ok(body) if body.len() as u64 <= MAX_SNAPSHOT_BYTES => body,
        _ => return (response_headers, StatusCode::BAD_GATEWAY).into_response(),
    };
    if serde_json::from_slice::<serde_json::Value>(&body).is_err() {
        return (response_headers, StatusCode::BAD_GATEWAY).into_response();
    }
    response_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    (response_headers, body).into_response()
}

pub(crate) async fn authorize_cluster_inventory(
    state: &AppState,
    request_headers: &HeaderMap,
) -> Result<HeaderMap, Response> {
    let Some(session) = read_session(request_headers, &state.sessions) else {
        return Err(unauthorized(state));
    };
    if session.is_expired(Utc::now().timestamp()) {
        return Err(unauthorized(state));
    }
    let session = crate::oauth::upstream_session(state, session)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "console session refresh failed");
            unauthorized(state)
        })?;
    let response_headers =
        response_session_headers(state, &session).map_err(|status| status.into_response())?;
    let upstream = state
        .http
        .get(state.config.cluster_authorization_url())
        .header(HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token)
        .send()
        .await
        .map_err(|error| {
            tracing::error!(%error, "console Cluster authorization upstream failed");
            (response_headers.clone(), StatusCode::BAD_GATEWAY).into_response()
        })?;
    match classify_snapshot_upstream(upstream.status()) {
        SnapshotUpstreamDisposition::Success => Ok(response_headers),
        SnapshotUpstreamDisposition::Unauthorized => Err(unauthorized(state)),
        SnapshotUpstreamDisposition::Forbidden => {
            Err((response_headers, StatusCode::FORBIDDEN).into_response())
        }
        SnapshotUpstreamDisposition::BadGateway => {
            tracing::warn!(status = %upstream.status(), "console Cluster authorization returned an error");
            Err((response_headers, StatusCode::BAD_GATEWAY).into_response())
        }
    }
}

fn classify_snapshot_upstream(status: reqwest::StatusCode) -> SnapshotUpstreamDisposition {
    match status {
        status if status.is_success() => SnapshotUpstreamDisposition::Success,
        reqwest::StatusCode::UNAUTHORIZED => SnapshotUpstreamDisposition::Unauthorized,
        reqwest::StatusCode::FORBIDDEN => SnapshotUpstreamDisposition::Forbidden,
        _ => SnapshotUpstreamDisposition::BadGateway,
    }
}

pub(crate) async fn cancel_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    request_headers: HeaderMap,
) -> Response {
    let Ok(task_id) = task_id.parse::<ProtocolTaskId>() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    proxy_json::<()>(
        &state,
        &request_headers,
        Method::POST,
        &format!("tasks/{task_id}/cancel"),
        None,
    )
    .await
}

pub(crate) async fn set_artifact_release_state(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    request_headers: HeaderMap,
    axum::Json(request): axum::Json<SetArtifactReleaseStateRequest>,
) -> Response {
    proxy_artifact_json(
        &state,
        &request_headers,
        Method::PUT,
        artifact_id,
        "release-state",
        Some(&request),
    )
    .await
}

pub(crate) async fn grant_artifact(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    request_headers: HeaderMap,
    axum::Json(request): axum::Json<PutGrantRequest>,
) -> Response {
    proxy_artifact_json(
        &state,
        &request_headers,
        Method::POST,
        artifact_id,
        "grants",
        Some(&request),
    )
    .await
}

pub(crate) async fn revoke_artifact_grant(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    request_headers: HeaderMap,
    axum::Json(request): axum::Json<AccessSubject>,
) -> Response {
    proxy_artifact_json(
        &state,
        &request_headers,
        Method::DELETE,
        artifact_id,
        "grants",
        Some(&request),
    )
    .await
}

pub(crate) async fn create_artifact_share_link(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    request_headers: HeaderMap,
    axum::Json(request): axum::Json<CreateArtifactShareLinkRequest>,
) -> Response {
    proxy_artifact_json(
        &state,
        &request_headers,
        Method::POST,
        artifact_id,
        "share-links",
        Some(&request),
    )
    .await
}

pub(crate) async fn revoke_artifact_share_link(
    State(state): State<AppState>,
    Path((artifact_id, link_id)): Path<(String, String)>,
    request_headers: HeaderMap,
) -> Response {
    let Ok(artifact_id) = ArtifactId::parse(artifact_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(link_id) = ArtifactShareLinkId::parse(link_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    proxy_json::<()>(
        &state,
        &request_headers,
        Method::DELETE,
        &format!("artifacts/{artifact_id}/share-links/{link_id}"),
        None,
    )
    .await
}

pub(crate) async fn create_artifact_access_request(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    request_headers: HeaderMap,
    axum::Json(request): axum::Json<CreateArtifactAccessRequest>,
) -> Response {
    proxy_artifact_json(
        &state,
        &request_headers,
        Method::POST,
        artifact_id,
        "access-requests",
        Some(&request),
    )
    .await
}

pub(crate) async fn list_artifact_access_requests(
    State(state): State<AppState>,
    Query(request): Query<ListArtifactAccessRequests>,
    request_headers: HeaderMap,
) -> Response {
    let query = artifact_access_request_query(&request);
    let path = if query.is_empty() {
        "artifact-access-requests".to_owned()
    } else {
        format!("artifact-access-requests?{query}")
    };
    proxy_json::<()>(&state, &request_headers, Method::GET, &path, None).await
}

pub(crate) async fn decide_artifact_access_request(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    request_headers: HeaderMap,
    axum::Json(decision): axum::Json<DecideArtifactAccessRequest>,
) -> Response {
    let Ok(request_id) = ArtifactAccessRequestId::parse(request_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    proxy_json(
        &state,
        &request_headers,
        Method::POST,
        &format!("artifact-access-requests/{request_id}/decision"),
        Some(&decision),
    )
    .await
}

pub(crate) async fn cancel_artifact_access_request(
    State(state): State<AppState>,
    Path(request_id): Path<String>,
    request_headers: HeaderMap,
) -> Response {
    let Ok(request_id) = ArtifactAccessRequestId::parse(request_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    proxy_json::<()>(
        &state,
        &request_headers,
        Method::POST,
        &format!("artifact-access-requests/{request_id}/cancel"),
        None,
    )
    .await
}

fn artifact_access_request_query(request: &ListArtifactAccessRequests) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    if let Some(scope) = request.scope {
        query.append_pair(
            "scope",
            match scope {
                ArtifactAccessRequestScope::Mine => "mine",
                ArtifactAccessRequestScope::Reviewable => "reviewable",
            },
        );
    }
    if let Some(state) = request.state {
        query.append_pair(
            "state",
            match state {
                ArtifactAccessRequestState::Pending => "pending",
                ArtifactAccessRequestState::Approved => "approved",
                ArtifactAccessRequestState::Denied => "denied",
                ArtifactAccessRequestState::Cancelled => "cancelled",
            },
        );
    }
    if let Some(cursor) = request.cursor {
        query.append_pair("cursor", &cursor.to_string());
    }
    if let Some(limit) = request.limit {
        query.append_pair("limit", &limit.to_string());
    }
    query.finish()
}

pub(crate) async fn download_artifact(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    request_headers: HeaderMap,
    method: Method,
) -> Response {
    let Ok(artifact_id) = ArtifactId::parse(artifact_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let session = match upstream_session(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let mut headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let mut request = state
        .stream_http
        .request(
            method,
            state.config.artifact_download_url(&artifact_id.to_string()),
        )
        .header(HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token);
    request = forward_read_headers(request, &request_headers);
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "console artifact download upstream failed");
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    if upstream.status() == reqwest::StatusCode::UNAUTHORIZED {
        return unauthorized(&state);
    }
    for name in [
        LOCATION,
        CONTENT_TYPE,
        CONTENT_LENGTH,
        CONTENT_RANGE,
        CONTENT_DISPOSITION,
        ACCEPT_RANGES,
        CACHE_CONTROL,
        ETAG,
        LAST_MODIFIED,
        REFERRER_POLICY,
        X_CONTENT_TYPE_OPTIONS,
        CONTENT_SECURITY_POLICY,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            headers.insert(name, value.clone());
        }
    }
    let status = upstream.status();
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

pub(crate) async fn preview_artifact(
    State(state): State<AppState>,
    Path(artifact_id): Path<String>,
    request_headers: HeaderMap,
    method: Method,
) -> Response {
    let Ok(artifact_id) = ArtifactId::parse(artifact_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let session = match upstream_session(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let mut headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let request = forward_read_headers(
        state
            .stream_http
            .request(
                method.clone(),
                state.config.artifact_download_url(&artifact_id.to_string()),
            )
            .header(HOST, state.config.gateway_host())
            .bearer_auth(&session.session.access_token),
        &request_headers,
    );
    let mut upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "console artifact preview authorization failed");
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    if upstream.status() == reqwest::StatusCode::UNAUTHORIZED {
        return unauthorized(&state);
    }
    if upstream.status().is_redirection() {
        let Some(location) = upstream
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
        else {
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        };
        let Ok(url) = url::Url::parse(location) else {
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        };
        if url.scheme() != "https" {
            tracing::warn!(
                scheme = url.scheme(),
                "rejected non-HTTPS artifact preview redirect"
            );
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        }
        let request =
            forward_read_headers(state.stream_http.request(method, url), &request_headers);
        upstream = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                tracing::error!(%error, "console artifact preview object read failed");
                return (headers, StatusCode::BAD_GATEWAY).into_response();
            }
        };
    }
    for name in [
        CONTENT_TYPE,
        CONTENT_LENGTH,
        CONTENT_RANGE,
        ACCEPT_RANGES,
        CACHE_CONTROL,
        ETAG,
        LAST_MODIFIED,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            headers.insert(name, value.clone());
        }
    }
    headers.insert(CONTENT_DISPOSITION, HeaderValue::from_static("inline"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    let status = upstream.status();
    let mut response = Response::new(Body::from_stream(upstream.bytes_stream()));
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}

fn forward_read_headers(
    mut request: reqwest::RequestBuilder,
    headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    for name in [
        RANGE,
        IF_RANGE,
        IF_MATCH,
        IF_NONE_MATCH,
        IF_MODIFIED_SINCE,
        IF_UNMODIFIED_SINCE,
    ] {
        if let Some(value) = headers.get(&name) {
            request = request.header(name, value);
        }
    }
    request
}

/// Margin before access-token expiry at which the proxied SSE stream is cut.
/// The browser's EventSource reconnects immediately and the new handler run
/// lands inside `ConsoleSession::should_refresh`'s 30 s window, so the token
/// is silently refreshed across reconnects.
const STREAM_TOKEN_MARGIN_SECS: i64 = 5;

pub(crate) async fn stream(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    request_headers: HeaderMap,
) -> Response {
    let session = match upstream_session(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let mut headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let mut url = state.config.admin_url("console/stream");
    if let Some(query) = query.as_deref() {
        url.set_query(Some(query));
    }
    let mut request = state
        .http
        .get(url)
        .header(HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token);
    if let Some(last_event_id) = request_headers.get("last-event-id") {
        request = request.header("last-event-id", last_event_id.clone());
    }
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "console stream upstream failed");
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    if upstream.status() == reqwest::StatusCode::UNAUTHORIZED {
        return unauthorized(&state);
    }
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    if !status.is_success() {
        return (headers, status).into_response();
    }
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/event-stream"));
    headers.insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    let remaining = session
        .session
        .access_expires_at
        .saturating_sub(STREAM_TOKEN_MARGIN_SECS)
        .saturating_sub(Utc::now().timestamp())
        .max(1);
    let deadline = tokio::time::sleep(std::time::Duration::from_secs(remaining.unsigned_abs()));
    let mut response = Response::new(Body::from_stream(
        upstream.bytes_stream().take_until(deadline),
    ));
    *response.status_mut() = StatusCode::OK;
    *response.headers_mut() = headers;
    response
}

async fn proxy_artifact_json<T: Serialize>(
    state: &AppState,
    request_headers: &HeaderMap,
    method: Method,
    artifact_id: String,
    suffix: &str,
    body: Option<&T>,
) -> Response {
    let Ok(artifact_id) = ArtifactId::parse(artifact_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    proxy_json(
        state,
        request_headers,
        method,
        &format!("artifacts/{artifact_id}/{suffix}"),
        body,
    )
    .await
}

async fn proxy_json<T: Serialize>(
    state: &AppState,
    request_headers: &HeaderMap,
    method: Method,
    path: &str,
    body: Option<&T>,
) -> Response {
    let session = match upstream_session(state, request_headers).await {
        Ok(session) => session,
        Err(response) => return response,
    };
    let mut headers = match response_session_headers(state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let mut request = state
        .http
        .request(method, state.config.admin_url(path))
        .header(HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token);
    if let Some(body) = body {
        request = request.json(body);
    }
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "console mutation upstream failed");
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    if upstream.status() == reqwest::StatusCode::UNAUTHORIZED {
        return unauthorized(state);
    }
    if upstream
        .content_length()
        .is_some_and(|length| length > MAX_SNAPSHOT_BYTES)
    {
        return (headers, StatusCode::BAD_GATEWAY).into_response();
    }
    let status = upstream.status();
    let content_type = upstream.headers().get(CONTENT_TYPE).cloned();
    let body = match upstream.bytes().await {
        Ok(body) if body.len() as u64 <= MAX_SNAPSHOT_BYTES => body,
        _ => return (headers, StatusCode::BAD_GATEWAY).into_response(),
    };
    if let Some(content_type) = content_type {
        headers.insert(CONTENT_TYPE, content_type);
    }
    (status, headers, body).into_response()
}

/// Session accessor for the apps host module; identical semantics to the
/// JSON proxies (cookie session, silent refresh, 401 on failure).
pub(crate) async fn upstream_session_for_apps(
    state: &AppState,
    request_headers: &HeaderMap,
) -> Result<crate::oauth::UpstreamSession, Response> {
    upstream_session(state, request_headers).await
}

async fn upstream_session(
    state: &AppState,
    request_headers: &HeaderMap,
) -> Result<crate::oauth::UpstreamSession, Response> {
    let Some(session) = read_session(request_headers, &state.sessions) else {
        return Err(unauthorized(state));
    };
    if session.is_expired(Utc::now().timestamp()) {
        return Err(unauthorized(state));
    }
    crate::oauth::upstream_session(state, session)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "console session refresh failed");
            unauthorized(state)
        })
}

pub(crate) fn response_session_headers(
    state: &AppState,
    session: &crate::oauth::UpstreamSession,
) -> Result<HeaderMap, StatusCode> {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    let value = HeaderValue::from_str(&session.session.csrf_token)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    headers.insert(CSRF_HEADER, value);
    if let Some((cookie, max_age)) = &session.replacement_cookie {
        set_session_cookie(&mut headers, cookie, *max_age, state.config.secure_cookie())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    Ok(headers)
}

fn constant_time_equal(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        difference |= usize::from(
            left.get(index).copied().unwrap_or_default()
                ^ right.get(index).copied().unwrap_or_default(),
        );
    }
    difference == 0
}

fn unauthorized(state: &AppState) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    clear_session_cookie(&mut headers, state.config.secure_cookie());
    (headers, StatusCode::UNAUTHORIZED).into_response()
}

#[cfg(test)]
mod tests {
    use super::{SnapshotUpstreamDisposition, classify_snapshot_upstream, constant_time_equal};

    #[test]
    fn csrf_comparison_rejects_wrong_values_and_lengths() {
        assert!(constant_time_equal("same", "same"));
        assert!(!constant_time_equal("same", "different"));
        assert!(!constant_time_equal("same", "sam"));
        assert!(!constant_time_equal("", "nonempty"));
    }

    #[test]
    fn snapshot_preserves_authentication_and_authorization_failures() {
        assert_eq!(
            classify_snapshot_upstream(reqwest::StatusCode::OK),
            SnapshotUpstreamDisposition::Success
        );
        assert_eq!(
            classify_snapshot_upstream(reqwest::StatusCode::UNAUTHORIZED),
            SnapshotUpstreamDisposition::Unauthorized
        );
        assert_eq!(
            classify_snapshot_upstream(reqwest::StatusCode::FORBIDDEN),
            SnapshotUpstreamDisposition::Forbidden
        );
        assert_eq!(
            classify_snapshot_upstream(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
            SnapshotUpstreamDisposition::BadGateway
        );
    }
}
