use axum::{
    body::Body,
    extract::{Path, Query, RawQuery, State},
    http::{
        HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode,
        header::{
            ACCEPT_RANGES, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE,
            CONTENT_SECURITY_POLICY, CONTENT_TYPE, ETAG, HOST, IF_MATCH, IF_MODIFIED_SINCE,
            IF_NONE_MATCH, IF_RANGE, IF_UNMODIFIED_SINCE, LAST_MODIFIED, RANGE, REFERRER_POLICY,
            X_CONTENT_TYPE_OPTIONS,
        },
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use futures::StreamExt;
use serde::Serialize;
use veoveo_mcp_contract::{
    AccessSubject, AgentInputRequestDecision, AgentOperatorMessageRequest, ArtifactAccessRequestId,
    ArtifactAccessRequestScope, ArtifactAccessRequestState, ArtifactId, ArtifactShareLinkId,
    CreateArtifactAccessRequest, CreateArtifactShareLinkRequest, DecideArtifactAccessRequest,
    ListArtifactAccessRequests, PutGrantRequest, SetArtifactReleaseStateRequest,
};

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
    let Ok(task_id) = uuid::Uuid::parse_str(&task_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if task_id.get_version_num() != 7 {
        return StatusCode::NOT_FOUND.into_response();
    }
    proxy_json::<()>(
        &state,
        &request_headers,
        Method::POST,
        &format!("tasks/{task_id}/cancel"),
        None,
    )
    .await
}

pub(crate) async fn send_agent_message(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    request_headers: HeaderMap,
    axum::Json(request): axum::Json<AgentOperatorMessageRequest>,
) -> Response {
    if !valid_agent_id(&agent_id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    proxy_json(
        &state,
        &request_headers,
        Method::POST,
        &format!("agents/{agent_id}/messages"),
        Some(&request),
    )
    .await
}

pub(crate) async fn read_agent_conversation(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    request_headers: HeaderMap,
) -> Response {
    if !valid_agent_id(&agent_id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    proxy_json::<()>(
        &state,
        &request_headers,
        Method::GET,
        &format!("agents/{agent_id}/conversation"),
        None,
    )
    .await
}

pub(crate) async fn list_agent_input_requests(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    request_headers: HeaderMap,
) -> Response {
    if !valid_agent_id(&agent_id) {
        return StatusCode::NOT_FOUND.into_response();
    }
    proxy_json::<()>(
        &state,
        &request_headers,
        Method::GET,
        &format!("agents/{agent_id}/input-requests"),
        None,
    )
    .await
}

pub(crate) async fn decide_agent_input_request(
    State(state): State<AppState>,
    Path((agent_id, input_request_id)): Path<(String, String)>,
    request_headers: HeaderMap,
    axum::Json(request): axum::Json<AgentInputRequestDecision>,
) -> Response {
    let Ok(input_request_id) = uuid::Uuid::parse_str(&input_request_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !valid_agent_id(&agent_id) || input_request_id.get_version_num() != 7 {
        return StatusCode::NOT_FOUND.into_response();
    }
    proxy_json(
        &state,
        &request_headers,
        Method::POST,
        &format!("agents/{agent_id}/input-requests/{input_request_id}/decision"),
        Some(&request),
    )
    .await
}

fn valid_agent_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
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
    let headers = match response_session_headers(&state, &session) {
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
    artifact_stream_response(upstream, headers, ArtifactStreamPresentation::Download)
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
    let headers = match response_session_headers(&state, &session) {
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
    let upstream = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::error!(%error, "console artifact preview authorization failed");
            return (headers, StatusCode::BAD_GATEWAY).into_response();
        }
    };
    if upstream.status() == reqwest::StatusCode::UNAUTHORIZED {
        return unauthorized(&state);
    }
    artifact_stream_response(upstream, headers, ArtifactStreamPresentation::Preview)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ArtifactStreamPresentation {
    Download,
    Preview,
}

fn artifact_stream_response(
    upstream: reqwest::Response,
    mut headers: HeaderMap,
    presentation: ArtifactStreamPresentation,
) -> Response {
    if upstream.status().is_redirection() {
        tracing::error!(
            status = %upstream.status(),
            ?presentation,
            "artifact upstream returned a forbidden redirect"
        );
        return (headers, StatusCode::BAD_GATEWAY).into_response();
    }
    for name in [
        CONTENT_TYPE,
        CONTENT_LENGTH,
        CONTENT_RANGE,
        ACCEPT_RANGES,
        CACHE_CONTROL,
        ETAG,
        LAST_MODIFIED,
        X_CONTENT_TYPE_OPTIONS,
    ] {
        if let Some(value) = upstream.headers().get(&name) {
            headers.insert(name, value.clone());
        }
    }
    match presentation {
        ArtifactStreamPresentation::Download => {
            for name in [
                CONTENT_DISPOSITION,
                REFERRER_POLICY,
                CONTENT_SECURITY_POLICY,
            ] {
                if let Some(value) = upstream.headers().get(&name) {
                    headers.insert(name, value.clone());
                }
            }
        }
        ArtifactStreamPresentation::Preview => {
            headers.insert(CONTENT_DISPOSITION, HeaderValue::from_static("inline"));
            headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
        }
    }
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
        .live_http
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

pub(crate) fn unauthorized(state: &AppState) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    clear_session_cookie(&mut headers, state.config.secure_cookie());
    (headers, StatusCode::UNAUTHORIZED).into_response()
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, convert::Infallible, sync::Arc, time::Duration};

    use axum::{
        Router,
        body::{Body, Bytes, to_bytes},
        extract::{RawQuery, State},
        http::{
            HeaderMap, HeaderValue, StatusCode,
            header::{COOKIE, HOST, LOCATION},
        },
        response::{IntoResponse, Redirect, Response, sse::Event, sse::Sse},
        routing::get,
    };
    use chrono::Utc;
    use futures::{StreamExt, stream as futures_stream};
    use tokio::{net::TcpListener, sync::Notify};
    use veoveo_mcp_contract::ScopeName;

    use super::{
        ArtifactStreamPresentation, SnapshotUpstreamDisposition, artifact_stream_response,
        classify_snapshot_upstream, constant_time_equal, stream,
    };
    use crate::{
        AppState,
        apps::AppTaskRegistry,
        config::Config,
        mcp_client::AuthScopedMcpClientPool,
        session::{ConsoleSession, SESSION_AAD, SESSION_COOKIE, SessionCipher},
    };

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

    #[tokio::test]
    async fn artifact_streaming_rejects_redirects_and_never_forwards_location() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/redirect", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/redirect",
                    get(|| async { Redirect::temporary("http://objects.invalid/private") }),
                ),
            )
            .await
            .unwrap();
        });
        let upstream = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap()
            .get(url)
            .send()
            .await
            .unwrap();

        let response = artifact_stream_response(
            upstream,
            HeaderMap::new(),
            ArtifactStreamPresentation::Download,
        );

        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert!(!response.headers().contains_key(LOCATION));
        server.abort();
    }

    #[tokio::test]
    async fn artifact_streaming_forwards_chunks_without_buffering_the_complete_object() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/artifact", listener.local_addr().unwrap());
        let release_second_chunk = Arc::new(Notify::new());
        let server_gate = Arc::clone(&release_second_chunk);
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route(
                    "/artifact",
                    get(move || {
                        let gate = Arc::clone(&server_gate);
                        async move {
                            let chunks = futures_stream::unfold(0_u8, move |state| {
                                let gate = Arc::clone(&gate);
                                async move {
                                    match state {
                                        0 => Some((
                                            Ok::<_, Infallible>(Bytes::from_static(b"first")),
                                            1,
                                        )),
                                        1 => {
                                            gate.notified().await;
                                            Some((
                                                Ok::<_, Infallible>(Bytes::from_static(b"second")),
                                                2,
                                            ))
                                        }
                                        _ => None,
                                    }
                                }
                            });
                            Response::new(Body::from_stream(chunks))
                        }
                    }),
                ),
            )
            .await
            .unwrap();
        });
        let upstream = reqwest::Client::new().get(url).send().await.unwrap();

        let response = artifact_stream_response(
            upstream,
            HeaderMap::new(),
            ArtifactStreamPresentation::Download,
        );
        let mut chunks = response.into_body().into_data_stream();

        assert_eq!(
            chunks.next().await.unwrap().unwrap(),
            Bytes::from_static(b"first")
        );
        release_second_chunk.notify_one();
        assert_eq!(
            chunks.next().await.unwrap().unwrap(),
            Bytes::from_static(b"second")
        );
        assert!(chunks.next().await.is_none());
        server.abort();
    }

    #[tokio::test]
    async fn console_event_stream_outlives_the_ordinary_request_timeout() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gateway_url =
            url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let gateway = Router::new().route(
            "/admin/admin/console/stream",
            get(|| async {
                let events = futures_stream::once(async {
                    tokio::time::sleep(Duration::from_millis(75)).await;
                    Ok::<_, Infallible>(Event::default().event("audit").data("{}"))
                });
                Sse::new(events).into_response()
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, gateway).await.unwrap();
        });

        let config = Arc::new(Config::for_test(gateway_url));
        let sessions = SessionCipher::new(config.session_key()).unwrap();
        let now = Utc::now().timestamp();
        let session = ConsoleSession {
            access_token: "access-token".to_owned(),
            access_expires_at: now + 300,
            refresh_token: "refresh-token".to_owned(),
            refresh_expires_at: now + 3_600,
            granted_scopes: BTreeSet::from([ScopeName::new("admin:manage").unwrap()]),
            csrf_token: "csrf-token".to_owned(),
        };
        let sealed = sessions.seal(&session, SESSION_AAD).unwrap();
        let mut request_headers = HeaderMap::new();
        request_headers.insert(
            COOKIE,
            HeaderValue::from_str(&format!("{SESSION_COOKIE}={sealed}")).unwrap(),
        );
        request_headers.insert(HOST, HeaderValue::from_static("console.test"));

        let state = AppState {
            config,
            http: reqwest::Client::builder()
                .timeout(Duration::from_millis(25))
                .build()
                .unwrap(),
            stream_http: reqwest::Client::new(),
            live_http: reqwest::Client::new(),
            cluster: None,
            sessions,
            mcp: Arc::new(AuthScopedMcpClientPool::new(&Default::default()).unwrap()),
            app_tasks: AppTaskRegistry::default(),
        };
        let response = stream(State(state), RawQuery(None), request_headers).await;
        let body = to_bytes(response.into_body(), 1_024).await.unwrap();

        let body = String::from_utf8_lossy(&body);
        assert!(
            body.contains("event: audit") && body.contains("data: {}"),
            "the delayed SSE event must survive the bounded JSON-client timeout; body={body:?}"
        );
        server.abort();
    }
}
