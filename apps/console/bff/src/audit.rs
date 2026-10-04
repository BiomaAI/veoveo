//! Cookie-authenticated forwarding for the fixed Console audit endpoints.
use crate::{
    AppState,
    api::{response_session_headers, unauthorized, upstream_session},
};
use axum::{
    body::Body,
    extract::{RawQuery, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures::StreamExt;
use veoveo_mcp_contract::audit::AuditPartition;

const MAX_PAGE_BYTES: usize = 8 * 1024 * 1024;
enum Endpoint {
    View(AuditPartition),
    Partitions,
    Records,
    Summary,
    Stream,
    Export,
}
impl Endpoint {
    fn path(&self) -> &'static str {
        match self {
            Self::View(_) => "console/audit/views",
            Self::Partitions => "console/audit/partitions",
            Self::Records => "console/audit/records",
            Self::Summary => "console/audit/summary",
            Self::Stream => "console/audit/stream",
            Self::Export => "console/audit/export",
        }
    }
    fn streaming(&self) -> bool {
        matches!(self, Self::Stream | Self::Export)
    }
}

pub(crate) async fn partitions(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    forward(state, headers, query, Endpoint::Partitions).await
}

pub(crate) async fn records(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    forward(state, headers, query, Endpoint::Records).await
}

pub(crate) async fn summary(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    forward(state, headers, query, Endpoint::Summary).await
}

pub(crate) async fn stream(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    forward(state, headers, query, Endpoint::Stream).await
}

pub(crate) async fn export(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    forward(state, headers, query, Endpoint::Export).await
}

pub(crate) async fn open_view(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(partition): axum::Json<AuditPartition>,
) -> Response {
    forward(state, headers, None, Endpoint::View(partition)).await
}

async fn forward(
    state: AppState,
    request_headers: HeaderMap,
    query: Option<String>,
    endpoint: Endpoint,
) -> Response {
    if query.as_ref().is_some_and(|query| query.len() > 65_536) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let session = match upstream_session(&state, &request_headers).await {
        Ok(session) => session,
        Err(response) => return *response,
    };
    let mut headers = match response_session_headers(&state, &session) {
        Ok(headers) => headers,
        Err(status) => return status.into_response(),
    };
    let mut url = state.config.admin_url(endpoint.path());
    url.set_query(query.as_deref());
    let http = if endpoint.streaming() {
        &state.live_http
    } else {
        &state.http
    };
    let mut request = match &endpoint {
        Endpoint::View(partition) => http.post(url).json(partition),
        _ => http.get(url),
    };
    request = request
        .header(header::HOST, state.config.gateway_host())
        .bearer_auth(&session.session.access_token);
    if matches!(endpoint, Endpoint::Stream)
        && let Some(cursor) = request_headers.get("last-event-id")
    {
        request = request.header("last-event-id", cursor.clone());
    }
    let mut upstream = match request.send().await {
        Ok(upstream) => upstream,
        Err(_) => return (headers, StatusCode::BAD_GATEWAY).into_response(),
    };
    if upstream.status() == reqwest::StatusCode::UNAUTHORIZED {
        return unauthorized(&state);
    }
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    for name in [header::CONTENT_TYPE, header::CONTENT_DISPOSITION] {
        if let Some(value) = upstream.headers().get(&name) {
            headers.insert(name, value.clone());
        }
    }
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    let body =
        if endpoint.streaming() && status.is_success() {
            headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
            let remaining = session
                .session
                .access_expires_at
                .saturating_sub(5)
                .saturating_sub(chrono::Utc::now().timestamp())
                .max(1);
            let stream = upstream.bytes_stream().take_until(tokio::time::sleep(
                std::time::Duration::from_secs(remaining.unsigned_abs()),
            ));
            Body::from_stream(stream)
        } else {
            if upstream
                .content_length()
                .is_some_and(|size| size > MAX_PAGE_BYTES as u64)
            {
                return (headers, StatusCode::BAD_GATEWAY).into_response();
            }
            let mut bytes = Vec::new();
            loop {
                match upstream.chunk().await {
                    Ok(Some(chunk)) if bytes.len() + chunk.len() <= MAX_PAGE_BYTES => {
                        bytes.extend_from_slice(&chunk)
                    }
                    Ok(None) => break,
                    _ => return (headers, StatusCode::BAD_GATEWAY).into_response(),
                }
            }
            Body::from(bytes)
        };
    let mut response = Response::new(body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    response
}
