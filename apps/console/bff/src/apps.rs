use std::{
    collections::VecDeque,
    convert::Infallible,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::{Query, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    response::{IntoResponse, Response, sse::Event, sse::Sse},
};
use serde::{Deserialize, Serialize};
use veoveo_mcp_apps_extension::{APP_MIME_TYPE, is_app_resource, resource_ui_meta, tool_app_link};
use veoveo_mcp_contract::{
    APP_RESOURCE_DEPENDENCIES_META_KEY, AppResourceDependency, AppResourceOperation,
};

use crate::{AppState, api, mcp_client::McpSession};

const MAX_APP_HTML_BYTES: usize = 2 * 1024 * 1024;
const MAX_CALL_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_CALL_RESULT_BYTES: usize = 2 * 1024 * 1024;

const MAX_TRACKED_APP_TASKS: usize = 1024;
const APP_TASK_RETENTION: Duration = Duration::from_secs(60 * 60);

const FRAME_CSP_OFFLINE: &str = "default-src 'none'; script-src 'unsafe-inline'; \
     style-src 'unsafe-inline'; img-src data: blob:; media-src blob:; \
     worker-src blob:; frame-ancestors 'self'; object-src 'none'; base-uri 'none'";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppCatalog {
    apps: Vec<AppDescriptor>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppDescriptor {
    server: String,
    resource_uri: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    /// Self-contained `data:` icon sources only — the console shell's CSP
    /// does not fetch remote images, and apps are self-contained by contract.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    icons: Vec<String>,
    tools: Vec<AppToolDescriptor>,
    resource_dependencies: Vec<AppResourceDependency>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppToolDescriptor {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    input_schema: serde_json::Value,
}

/// The server owning a projected app URI is its first path segment:
/// `ui://{server}/{page}` (guaranteed by the gateway's ServerOwned
/// projection).
fn app_uri_server(uri: &str) -> Option<&str> {
    let path = uri.strip_prefix("ui://")?;
    if uri.contains("..") {
        return None;
    }
    let (server, page) = path.split_once('/')?;
    (!server.is_empty() && !page.is_empty()).then_some(server)
}

/// A view may read only resources its own server owns: the server's
/// `{server}://…` scheme, or another of the server's projected `ui://…`
/// views. Gateway policy remains the authoritative second wall.
fn app_resource_uri_allowed(server: &str, uri: &str) -> bool {
    if uri.contains("..") {
        return false;
    }
    let own_scheme = uri
        .strip_prefix(server)
        .and_then(|rest| rest.strip_prefix("://"))
        .is_some_and(|rest| !rest.is_empty());
    own_scheme || app_uri_server(uri) == Some(server)
}

fn app_resource_dependencies(resource: &rmcp::model::Resource) -> Vec<AppResourceDependency> {
    resource
        .meta
        .as_ref()
        .and_then(|meta| meta.0.get(APP_RESOURCE_DEPENDENCIES_META_KEY))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}

fn app_dependency_allows_resource(resource: &rmcp::model::Resource, uri: &str) -> bool {
    if uri.contains("..") {
        return false;
    }
    app_resource_dependencies(resource)
        .iter()
        .any(|dependency| {
            dependency.operations.contains(&AppResourceOperation::Read)
                && uri.starts_with(dependency.uri_prefix.as_str())
                && uri
                    .strip_prefix(dependency.scheme.as_str())
                    .and_then(|rest| rest.strip_prefix("://"))
                    .is_some_and(|rest| !rest.is_empty())
        })
}

struct AppTaskOwner {
    server: String,
    app_uri: String,
    recorded_at: Instant,
}

/// Task-augmented calls made through the apps proxy, in insertion order so
/// the oldest entry falls out first. Only the view that created a task may
/// poll or cancel it; gateway task policy remains the authoritative second
/// wall. Bounded, with lazy expiry on access — no background sweeper.
#[derive(Clone, Default)]
pub(crate) struct AppTaskRegistry(Arc<Mutex<VecDeque<(String, AppTaskOwner)>>>);

impl AppTaskRegistry {
    pub(crate) fn record(&self, task_id: &str, server: &str, app_uri: &str) {
        self.record_at(Instant::now(), task_id, server, app_uri);
    }

    fn record_at(&self, now: Instant, task_id: &str, server: &str, app_uri: &str) {
        let Ok(mut tasks) = self.0.lock() else {
            return;
        };
        evict_expired_app_tasks(&mut tasks, now);
        tasks.retain(|(id, _)| id != task_id);
        while tasks.len() >= MAX_TRACKED_APP_TASKS {
            tasks.pop_front();
        }
        tasks.push_back((
            task_id.to_owned(),
            AppTaskOwner {
                server: server.to_owned(),
                app_uri: app_uri.to_owned(),
                recorded_at: now,
            },
        ));
    }

    pub(crate) fn owns(&self, task_id: &str, server: &str, app_uri: &str) -> bool {
        self.owns_at(Instant::now(), task_id, server, app_uri)
    }

    fn owns_at(&self, now: Instant, task_id: &str, server: &str, app_uri: &str) -> bool {
        let Ok(mut tasks) = self.0.lock() else {
            return false;
        };
        evict_expired_app_tasks(&mut tasks, now);
        tasks
            .iter()
            .any(|(id, owner)| id == task_id && owner.server == server && owner.app_uri == app_uri)
    }
}

fn evict_expired_app_tasks(tasks: &mut VecDeque<(String, AppTaskOwner)>, now: Instant) {
    while let Some((_, owner)) = tasks.front() {
        if now.saturating_duration_since(owner.recorded_at) < APP_TASK_RETENTION {
            return;
        }
        tasks.pop_front();
    }
}

/// Transport-level failures mean the pooled session's connection is gone
/// (rmcp's single-attempt expired-session recovery has already run inside
/// the transport); a server-side `McpError` means the session is healthy
/// and retrying would re-execute work.
fn is_transport_error(error: &rmcp::ServiceError) -> bool {
    matches!(
        error,
        rmcp::ServiceError::TransportSend(_) | rmcp::ServiceError::TransportClosed
    )
}

struct AppsSessionOutcome<T> {
    session: McpSession,
    response_headers: HeaderMap,
    access_expires_at: i64,
    result: Result<T, rmcp::ServiceError>,
}

fn with_session_headers(mut response: Response, headers: HeaderMap) -> Response {
    response.headers_mut().extend(headers);
    response
}

/// Run `operation` against the pooled gateway MCP session, rebuilding the
/// session and retrying once when the transport is dead (e.g. the gateway
/// restarted and discarded every session). Returns the session actually
/// used so callers can issue follow-up calls without re-entering the pool.
async fn with_apps_session<T, F>(
    state: &AppState,
    request_headers: &HeaderMap,
    operation: impl Fn(McpSession) -> F,
) -> Result<AppsSessionOutcome<T>, Response>
where
    F: Future<Output = Result<T, rmcp::ServiceError>>,
{
    let upstream = api::upstream_session_for_apps(state, request_headers).await?;
    let response_headers =
        api::response_session_headers(state, &upstream).map_err(IntoResponse::into_response)?;
    let mut retried = false;
    loop {
        let mcp = state
            .mcp
            .session(
                &state.config,
                &upstream.session.access_token,
                upstream.session.access_expires_at,
            )
            .await
            .map_err(|error| {
                tracing::error!(%error, "console apps MCP session failed");
                StatusCode::BAD_GATEWAY.into_response()
            })?;
        match operation(mcp.clone()).await {
            Err(error) if is_transport_error(&error) && !retried => {
                retried = true;
                tracing::warn!(
                    %error,
                    "console apps MCP transport failed; retrying on a fresh session"
                );
                state
                    .mcp
                    .invalidate(&upstream.session.access_token, &mcp)
                    .await;
            }
            result => {
                return Ok(AppsSessionOutcome {
                    session: mcp,
                    response_headers,
                    access_expires_at: upstream.session.access_expires_at,
                    result,
                });
            }
        }
    }
}

pub(crate) async fn list_apps(
    State(state): State<AppState>,
    request_headers: HeaderMap,
) -> Response {
    let listing = with_apps_session(&state, &request_headers, |mcp| async move {
        mcp.app_catalog().await
    })
    .await;
    let AppsSessionOutcome {
        response_headers,
        result,
        ..
    } = match listing {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    let catalog = match result {
        Ok(listing) => listing,
        Err(error) => {
            tracing::error!(%error, "console apps listing failed");
            return with_session_headers(StatusCode::BAD_GATEWAY.into_response(), response_headers);
        }
    };
    let mut apps = Vec::new();
    for resource in catalog
        .resources()
        .iter()
        .filter(|resource| is_app_resource(resource))
    {
        let Some(server) = app_uri_server(&resource.uri) else {
            continue;
        };
        let tools = catalog
            .tools()
            .iter()
            .filter_map(|tool| {
                let link = tool_app_link(tool)?;
                if !link.visible_to_app() || link.resource_uri != resource.uri {
                    return None;
                }
                let local = tool.name.strip_prefix(&format!("{server}__"))?;
                Some(AppToolDescriptor {
                    name: local.to_owned(),
                    title: tool.title.clone(),
                    description: tool.description.as_deref().map(ToOwned::to_owned),
                    input_schema: serde_json::Value::Object(
                        tool.input_schema.as_ref().clone().into_iter().collect(),
                    ),
                })
            })
            .collect();
        apps.push(AppDescriptor {
            server: server.to_owned(),
            resource_uri: resource.uri.clone(),
            name: resource.name.clone(),
            title: resource.title.clone(),
            description: resource.description.as_deref().map(ToOwned::to_owned),
            icons: resource
                .icons
                .iter()
                .flatten()
                .filter(|icon| icon.src.starts_with("data:image/"))
                .map(|icon| icon.src.clone())
                .collect(),
            tools,
            resource_dependencies: app_resource_dependencies(resource),
        });
    }
    with_session_headers(Json(AppCatalog { apps }).into_response(), response_headers)
}

#[derive(Deserialize)]
pub(crate) struct FrameQuery {
    uri: String,
}

pub(crate) async fn app_frame(
    State(state): State<AppState>,
    Query(query): Query<FrameQuery>,
    request_headers: HeaderMap,
) -> Response {
    if app_uri_server(&query.uri).is_none() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let uri = query.uri.clone();
    let read = with_apps_session(&state, &request_headers, |mcp| {
        let uri = uri.clone();
        async move {
            let catalog = mcp.app_catalog().await?;
            let resource = catalog
                .resources()
                .iter()
                .find(|resource| resource.uri == uri && is_app_resource(resource));
            let contents = mcp
                .read_resource(rmcp::model::ReadResourceRequestParams::new(uri))
                .await?;
            Ok((resource.cloned(), contents))
        }
    })
    .await;
    let AppsSessionOutcome {
        response_headers,
        result: read,
        ..
    } = match read {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    let (resource, result) = match read {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(%error, uri = %query.uri, "console app frame read failed");
            return with_session_headers(StatusCode::NOT_FOUND.into_response(), response_headers);
        }
    };
    let Some(resource) = resource else {
        return with_session_headers(StatusCode::NOT_FOUND.into_response(), response_headers);
    };
    let Some(html) = result.contents.iter().find_map(|contents| match contents {
        rmcp::model::ResourceContents::TextResourceContents {
            text, mime_type, ..
        } if mime_type.as_deref() == Some(APP_MIME_TYPE) => Some(text.clone()),
        _ => None,
    }) else {
        return with_session_headers(StatusCode::NOT_FOUND.into_response(), response_headers);
    };
    if html.len() > MAX_APP_HTML_BYTES {
        return with_session_headers(StatusCode::BAD_GATEWAY.into_response(), response_headers);
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    let Ok(frame_csp) = frame_csp(&resource) else {
        tracing::warn!(uri = %query.uri, "console rejected invalid MCP App CSP");
        return with_session_headers(StatusCode::BAD_GATEWAY.into_response(), response_headers);
    };
    headers.insert("content-security-policy", frame_csp);
    headers.insert("x-frame-options", HeaderValue::from_static("SAMEORIGIN"));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    with_session_headers(
        (StatusCode::OK, headers, html).into_response(),
        response_headers,
    )
}

fn frame_csp(resource: &rmcp::model::Resource) -> Result<HeaderValue, ()> {
    let Some(metadata) = resource_ui_meta(resource) else {
        return HeaderValue::from_str(FRAME_CSP_OFFLINE).map_err(|_| ());
    };
    let Some(csp) = metadata.csp else {
        return HeaderValue::from_str(FRAME_CSP_OFFLINE).map_err(|_| ());
    };
    let connect = csp_sources(&csp.connect_domains, &["http", "https", "ws", "wss"])?;
    let resources = csp_sources(&csp.resource_domains, &["http", "https"])?;
    let frames = csp_sources(&csp.frame_domains, &["http", "https"])?;
    let bases = csp_sources(&csp.base_uri_domains, &["http", "https"])?;
    let resource_sources = if resources.is_empty() {
        String::new()
    } else {
        format!(" {}", resources.join(" "))
    };
    let font_sources = if resources.is_empty() {
        "'none'".to_owned()
    } else {
        resources.join(" ")
    };
    let mut policy = format!(
        "default-src 'none'; script-src 'unsafe-inline'{resource_sources}; \
         style-src 'unsafe-inline'{resource_sources}; \
         img-src data: blob:{resource_sources}; media-src blob:{resource_sources}; \
         font-src {font_sources}; worker-src blob:; object-src 'none'; \
         frame-ancestors 'self'"
    );
    if !connect.is_empty() {
        policy.push_str("; connect-src ");
        policy.push_str(&connect.join(" "));
    }
    if !frames.is_empty() {
        policy.push_str("; frame-src ");
        policy.push_str(&frames.join(" "));
    }
    policy.push_str("; base-uri ");
    let base_sources = bases.join(" ");
    policy.push_str(if bases.is_empty() {
        "'none'"
    } else {
        &base_sources
    });
    HeaderValue::from_str(&policy).map_err(|_| ())
}

fn csp_sources(values: &[String], allowed_schemes: &[&str]) -> Result<Vec<String>, ()> {
    let mut sources = Vec::with_capacity(values.len());
    for value in values {
        let url = url::Url::parse(value).map_err(|_| ())?;
        if !allowed_schemes.contains(&url.scheme())
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || !matches!(url.path(), "" | "/")
        {
            return Err(());
        }
        let mut source = format!("{}://{}", url.scheme(), url.host_str().ok_or(())?);
        if let Some(port) = url.port() {
            source.push(':');
            source.push_str(&port.to_string());
        }
        sources.push(source);
    }
    sources.sort();
    sources.dedup();
    Ok(sources)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadAppResourceRequest {
    server: String,
    app_uri: String,
    uri: String,
}

/// `resources/read` proxied for an app view. Own-server resources remain
/// implicit. A foreign resource must match one gateway-projected dependency
/// on the exact App resource under the active profile and authority.
pub(crate) async fn read_app_resource(
    State(state): State<AppState>,
    request_headers: HeaderMap,
    Json(request): Json<ReadAppResourceRequest>,
) -> Response {
    if app_uri_server(&request.app_uri) != Some(request.server.as_str()) {
        return call_error(
            StatusCode::BAD_REQUEST,
            "app uri does not belong to the server",
        );
    }
    if request.uri.contains("..") {
        return call_error(StatusCode::FORBIDDEN, "resource URI is not admitted");
    }
    let server = request.server.clone();
    let app_uri = request.app_uri.clone();
    let uri = request.uri.as_str();
    let read = with_apps_session(&state, &request_headers, |mcp| {
        let server = server.clone();
        let app_uri = app_uri.clone();
        async move {
            let catalog = mcp.app_catalog().await?;
            let app_resource = catalog.resources().iter().find(|resource| {
                resource.uri == app_uri
                    && is_app_resource(resource)
                    && app_uri_server(&resource.uri) == Some(server.as_str())
            });
            let Some(app_resource) = app_resource else {
                return Ok(None);
            };
            if !app_resource_uri_allowed(&server, uri)
                && !app_dependency_allows_resource(app_resource, uri)
            {
                return Ok(None);
            }
            mcp.read_resource(rmcp::model::ReadResourceRequestParams::new(uri))
                .await
                .map(Some)
        }
    })
    .await;
    let AppsSessionOutcome {
        response_headers,
        result: read,
        ..
    } = match read {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    let result = match read {
        Ok(Some(result)) => result,
        Ok(None) => {
            return with_session_headers(
                call_error(
                    StatusCode::FORBIDDEN,
                    "resource is not declared for this App",
                ),
                response_headers,
            );
        }
        Err(error) => {
            return with_session_headers(
                call_error(
                    StatusCode::BAD_GATEWAY,
                    &format!("resource read failed: {error}"),
                ),
                response_headers,
            );
        }
    };
    let Ok(body) = serde_json::to_vec(&result) else {
        return with_session_headers(StatusCode::BAD_GATEWAY.into_response(), response_headers);
    };
    if body.len() > MAX_CALL_RESULT_BYTES {
        return with_session_headers(
            call_error(StatusCode::BAD_GATEWAY, "resource read exceeds the cap"),
            response_headers,
        );
    }
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    with_session_headers(
        (StatusCode::OK, headers, body).into_response(),
        response_headers,
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppResourceEventsQuery {
    server: String,
    app_uri: String,
    uri: String,
    subscription_id: uuid::Uuid,
}

/// Project one authorized upstream MCP resource subscription into a
/// contentless browser SSE wake stream. The App still reads current state
/// through `resources/read`; notifications never carry domain payloads.
pub(crate) async fn app_resource_events(
    State(state): State<AppState>,
    request_headers: HeaderMap,
    Query(query): Query<AppResourceEventsQuery>,
) -> Response {
    if app_uri_server(&query.app_uri) != Some(query.server.as_str()) {
        return call_error(
            StatusCode::BAD_REQUEST,
            "app URI does not belong to the server",
        );
    }
    if !app_resource_uri_allowed(&query.server, &query.uri) {
        return call_error(
            StatusCode::FORBIDDEN,
            "resource subscription is not owned by this App's server",
        );
    }
    let listing = with_apps_session(&state, &request_headers, |mcp| async move {
        mcp.app_catalog().await
    })
    .await;
    let AppsSessionOutcome {
        session,
        response_headers,
        access_expires_at,
        result,
    } = match listing {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    let catalog = match result {
        Ok(catalog) => catalog,
        Err(error) => {
            tracing::error!(%error, "console App resource subscription catalog failed");
            return with_session_headers(StatusCode::BAD_GATEWAY.into_response(), response_headers);
        }
    };
    if !catalog.resources().iter().any(|resource| {
        resource.uri == query.app_uri
            && is_app_resource(resource)
            && app_uri_server(&resource.uri) == Some(query.server.as_str())
    }) {
        return with_session_headers(
            call_error(StatusCode::FORBIDDEN, "App resource is not available"),
            response_headers,
        );
    }
    let receiver = match session
        .subscribe_app_resource(query.subscription_id, query.uri.clone())
        .await
    {
        Ok(receiver) => receiver,
        Err(error) => {
            tracing::error!(%error, uri = %query.uri, "console App resource subscription failed");
            return with_session_headers(
                call_error(StatusCode::BAD_GATEWAY, "resource subscription failed"),
                response_headers,
            );
        }
    };
    let remaining = access_expires_at
        .saturating_sub(5)
        .saturating_sub(chrono::Utc::now().timestamp())
        .max(1);
    let deadline = Box::pin(tokio::time::sleep(Duration::from_secs(
        remaining.unsigned_abs(),
    )));
    let stream = futures::stream::unfold(
        (receiver, query.uri, deadline, true),
        |(mut receiver, uri, mut deadline, initial)| async move {
            if initial {
                return Some((
                    Ok::<Event, Infallible>(Event::default().event("subscribed").data("{}")),
                    (receiver, uri, deadline, false),
                ));
            }
            loop {
                let updated = tokio::select! {
                    _ = &mut deadline => return None,
                    updated = receiver.recv() => updated,
                };
                match updated {
                    Ok(updated) if updated == uri => {
                        let event = Event::default()
                            .event("resource-updated")
                            .json_data(serde_json::json!({"uri": updated}))
                            .expect("resource update URI serializes");
                        return Some((
                            Ok::<Event, Infallible>(event),
                            (receiver, uri, deadline, false),
                        ));
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        let event = Event::default()
                            .event("resource-updated")
                            .json_data(serde_json::json!({"uri": uri}))
                            .expect("resource update URI serializes");
                        return Some((
                            Ok::<Event, Infallible>(event),
                            (receiver, uri, deadline, false),
                        ));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    );
    let mut response = Sse::new(stream).into_response();
    response.headers_mut().extend(response_headers);
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert("x-accel-buffering", HeaderValue::from_static("no"));
    response
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnsubscribeAppResourceRequest {
    subscription_id: uuid::Uuid,
}

pub(crate) async fn unsubscribe_app_resource(
    State(state): State<AppState>,
    request_headers: HeaderMap,
    Json(request): Json<UnsubscribeAppResourceRequest>,
) -> Response {
    let listing = with_apps_session(&state, &request_headers, |mcp| async move {
        mcp.app_catalog().await
    })
    .await;
    let AppsSessionOutcome {
        session,
        response_headers,
        result,
        ..
    } = match listing {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    if let Err(error) = result {
        tracing::error!(%error, "console App unsubscribe catalog failed");
        return with_session_headers(StatusCode::BAD_GATEWAY.into_response(), response_headers);
    }
    let response = match session
        .unsubscribe_app_resource(request.subscription_id)
        .await
    {
        Ok(()) => {
            capped_json_response(&serde_json::json!({}), "unsubscribe result exceeds the cap")
        }
        Err(error) => {
            tracing::error!(%error, "console App resource unsubscribe failed");
            call_error(StatusCode::BAD_GATEWAY, "resource unsubscribe failed")
        }
    };
    with_session_headers(response, response_headers)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CallAppToolRequest {
    server: String,
    app_uri: String,
    tool: String,
    #[serde(default)]
    arguments: serde_json::Value,
    #[serde(default)]
    task: Option<rmcp::model::TaskMetadata>,
}

#[derive(Serialize)]
struct CallAppToolError {
    error: String,
}

fn call_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Json(CallAppToolError {
            error: message.to_owned(),
        }),
    )
        .into_response()
}

fn capped_json_response<T: Serialize>(result: &T, cap_message: &str) -> Response {
    let Ok(body) = serde_json::to_vec(result) else {
        return StatusCode::BAD_GATEWAY.into_response();
    };
    if body.len() > MAX_CALL_RESULT_BYTES {
        return call_error(StatusCode::BAD_GATEWAY, cap_message);
    }
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    (StatusCode::OK, headers, body).into_response()
}

pub(crate) async fn call_app_tool(
    State(state): State<AppState>,
    request_headers: HeaderMap,
    Json(request): Json<CallAppToolRequest>,
) -> Response {
    // An app view may only call app-visible tools of its own server, linked
    // to its own view; gateway policy remains the authoritative second wall.
    if request.tool.contains("__") {
        return call_error(StatusCode::BAD_REQUEST, "tool must be a local tool name");
    }
    if app_uri_server(&request.app_uri) != Some(request.server.as_str()) {
        return call_error(
            StatusCode::BAD_REQUEST,
            "app uri does not belong to the server",
        );
    }
    let argument_bytes = request.arguments.to_string().len();
    if argument_bytes > MAX_CALL_ARGUMENT_BYTES {
        return call_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "tool arguments exceed the cap",
        );
    }
    let gateway_tool = format!("{}__{}", request.server, request.tool);
    let listing = with_apps_session(&state, &request_headers, |mcp| async move {
        mcp.app_catalog().await
    })
    .await;
    // The tool call below deliberately stays single-shot on the session the
    // listing just proved healthy: tool calls are not idempotent, so only
    // rmcp's own in-transport replay may retry them.
    let AppsSessionOutcome {
        session: mcp,
        response_headers,
        result: listing,
        ..
    } = match listing {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    let catalog = match listing {
        Ok(catalog) => catalog,
        Err(error) => {
            tracing::error!(%error, "console apps tool listing failed");
            return with_session_headers(StatusCode::BAD_GATEWAY.into_response(), response_headers);
        }
    };
    let Some(tool) = catalog
        .tools()
        .iter()
        .find(|tool| tool.name.as_ref() == gateway_tool)
    else {
        return with_session_headers(
            call_error(StatusCode::NOT_FOUND, "unknown tool for this app"),
            response_headers,
        );
    };
    let allowed = tool_app_link(tool)
        .is_some_and(|link| link.visible_to_app() && link.resource_uri == request.app_uri);
    if !allowed {
        return with_session_headers(
            call_error(
                StatusCode::FORBIDDEN,
                "tool is not app-visible for this view",
            ),
            response_headers,
        );
    }
    let mut params = rmcp::model::CallToolRequestParams::new(gateway_tool);
    match request.arguments {
        serde_json::Value::Object(map) => {
            params = params.with_arguments(map.into_iter().collect());
        }
        serde_json::Value::Null => {}
        _ => {
            return with_session_headers(
                call_error(StatusCode::BAD_REQUEST, "arguments must be a JSON object"),
                response_headers,
            );
        }
    }
    if let Some(task) = request.task {
        // The typed `call_tool` helper only accepts a `CallToolResult`; a
        // task-augmented call answers with a `CreateTaskResult`, so it goes
        // through the generic request path.
        let result = match mcp
            .send_request(rmcp::model::ClientRequest::CallToolRequest(
                rmcp::model::CallToolRequest::new(params.with_task(task)),
            ))
            .await
        {
            Ok(rmcp::model::ServerResult::CreateTaskResult(result)) => result,
            Ok(_) => {
                return with_session_headers(
                    call_error(
                        StatusCode::BAD_GATEWAY,
                        "task-augmented call returned an unexpected result",
                    ),
                    response_headers,
                );
            }
            Err(error) => {
                return with_session_headers(
                    call_error(
                        StatusCode::BAD_GATEWAY,
                        &format!("tool call failed: {error}"),
                    ),
                    response_headers,
                );
            }
        };
        state
            .app_tasks
            .record(&result.task.task_id, &request.server, &request.app_uri);
        return with_session_headers(
            capped_json_response(&result, "tool result exceeds the cap"),
            response_headers,
        );
    }
    let result = match mcp.call_tool(params).await {
        Ok(result) => result,
        Err(error) => {
            return with_session_headers(
                call_error(
                    StatusCode::BAD_GATEWAY,
                    &format!("tool call failed: {error}"),
                ),
                response_headers,
            );
        }
    };
    with_session_headers(
        capped_json_response(&result, "tool result exceeds the cap"),
        response_headers,
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppTaskRequest {
    server: String,
    app_uri: String,
    task_id: String,
}

/// The allowlist mirrors `call_app_tool`: the view must belong to the named
/// server, and only the view whose task-augmented call created the task may
/// poll or cancel it.
fn app_task_access_denied(state: &AppState, request: &AppTaskRequest) -> Option<Response> {
    if app_uri_server(&request.app_uri) != Some(request.server.as_str()) {
        return Some(call_error(
            StatusCode::BAD_REQUEST,
            "app uri does not belong to the server",
        ));
    }
    if !state
        .app_tasks
        .owns(&request.task_id, &request.server, &request.app_uri)
    {
        return Some(call_error(
            StatusCode::NOT_FOUND,
            "unknown task for this app",
        ));
    }
    None
}

pub(crate) async fn get_app_task(
    State(state): State<AppState>,
    request_headers: HeaderMap,
    Json(request): Json<AppTaskRequest>,
) -> Response {
    if let Some(response) = app_task_access_denied(&state, &request) {
        return response;
    }
    let task_id = request.task_id.as_str();
    let outcome = with_apps_session(&state, &request_headers, |mcp| async move {
        mcp.send_request(rmcp::model::ClientRequest::GetTaskRequest(
            rmcp::model::GetTaskRequest::new(rmcp::model::GetTaskParams::new(task_id)),
        ))
        .await
    })
    .await;
    let AppsSessionOutcome {
        response_headers,
        result,
        ..
    } = match outcome {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    let response = match result {
        Ok(rmcp::model::ServerResult::GetTaskResult(result)) => {
            capped_json_response(&result, "task status exceeds the cap")
        }
        Ok(_) => call_error(
            StatusCode::BAD_GATEWAY,
            "task get returned an unexpected result",
        ),
        Err(error) => call_error(
            StatusCode::BAD_GATEWAY,
            &format!("task get failed: {error}"),
        ),
    };
    with_session_headers(response, response_headers)
}

pub(crate) async fn get_app_task_result(
    State(state): State<AppState>,
    request_headers: HeaderMap,
    Json(request): Json<AppTaskRequest>,
) -> Response {
    if let Some(response) = app_task_access_denied(&state, &request) {
        return response;
    }
    let task_id = request.task_id.as_str();
    let outcome = with_apps_session(&state, &request_headers, |mcp| async move {
        mcp.send_request(rmcp::model::ClientRequest::GetTaskPayloadRequest(
            rmcp::model::GetTaskPayloadRequest::new(rmcp::model::GetTaskPayloadParams::new(
                task_id,
            )),
        ))
        .await
    })
    .await;
    let AppsSessionOutcome {
        response_headers,
        result,
        ..
    } = match outcome {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    let response = match result {
        // Per spec the payload takes the shape of the original request's
        // result (`CallToolResult` for the calls this registry admits), so
        // whichever untagged variant it decoded into is serialized back
        // unchanged.
        Ok(result) => capped_json_response(&result, "task result exceeds the cap"),
        Err(error) => call_error(
            StatusCode::BAD_GATEWAY,
            &format!("task result failed: {error}"),
        ),
    };
    with_session_headers(response, response_headers)
}

pub(crate) async fn cancel_app_task(
    State(state): State<AppState>,
    request_headers: HeaderMap,
    Json(request): Json<AppTaskRequest>,
) -> Response {
    if let Some(response) = app_task_access_denied(&state, &request) {
        return response;
    }
    let task_id = request.task_id.as_str();
    let outcome = with_apps_session(&state, &request_headers, |mcp| async move {
        mcp.send_request(rmcp::model::ClientRequest::CancelTaskRequest(
            rmcp::model::CancelTaskRequest::new(rmcp::model::CancelTaskParams::new(task_id)),
        ))
        .await
    })
    .await;
    // The registry entry deliberately stays: polling after a cancel keeps
    // observing the terminal status until the entry expires.
    let AppsSessionOutcome {
        response_headers,
        result,
        ..
    } = match outcome {
        Ok(outcome) => outcome,
        Err(response) => return response,
    };
    let response = match result {
        // `CancelTaskResult` shares `GetTaskResult`'s wire shape, so the
        // untagged decode lands on the latter.
        Ok(rmcp::model::ServerResult::GetTaskResult(result)) => {
            capped_json_response(&result, "task status exceeds the cap")
        }
        Ok(rmcp::model::ServerResult::CancelTaskResult(result)) => {
            capped_json_response(&result, "task status exceeds the cap")
        }
        Ok(_) => call_error(
            StatusCode::BAD_GATEWAY,
            "task cancel returned an unexpected result",
        ),
        Err(error) => call_error(
            StatusCode::BAD_GATEWAY,
            &format!("task cancel failed: {error}"),
        ),
    };
    with_session_headers(response, response_headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_transport_failures_are_safe_to_retry() {
        assert!(is_transport_error(&rmcp::ServiceError::TransportClosed));
        assert!(is_transport_error(&rmcp::ServiceError::TransportSend(
            rmcp::transport::DynamicTransportError::from_parts(
                "test",
                std::any::TypeId::of::<()>(),
                Box::new(std::io::Error::other("connection lost")),
            ),
        )));
        assert!(!is_transport_error(&rmcp::ServiceError::UnexpectedResponse));
        assert!(!is_transport_error(&rmcp::ServiceError::Cancelled {
            reason: Some("caller cancelled the operation".to_owned()),
        }));
    }

    #[test]
    fn app_responses_preserve_rotated_session_headers() {
        let mut session_headers = HeaderMap::new();
        session_headers.insert(
            "set-cookie",
            HeaderValue::from_static("__Host-veoveo-console=rotated"),
        );
        session_headers.insert(
            "x-veoveo-csrf-token",
            HeaderValue::from_static("rotated-csrf"),
        );
        let response = with_session_headers(
            (
                StatusCode::OK,
                [(CONTENT_TYPE, HeaderValue::from_static("application/json"))],
                "{}",
            )
                .into_response(),
            session_headers,
        );

        assert_eq!(
            response.headers().get("set-cookie").unwrap(),
            "__Host-veoveo-console=rotated"
        );
        assert_eq!(
            response.headers().get("x-veoveo-csrf-token").unwrap(),
            "rotated-csrf"
        );
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[test]
    fn app_uri_ownership_is_the_first_path_segment() {
        assert_eq!(
            app_uri_server("ui://timeseries/forecast.html"),
            Some("timeseries")
        );
        assert_eq!(
            app_uri_server("ui://charts/views/main.html"),
            Some("charts")
        );
        assert_eq!(app_uri_server("ui://timeseries"), None);
        assert_eq!(app_uri_server("ui://timeseries/"), None);
        assert_eq!(app_uri_server("ui:///page.html"), None);
        assert_eq!(app_uri_server("timeseries://artifact/x"), None);
        assert_eq!(app_uri_server("ui://timeseries/../admin.html"), None);
    }

    #[test]
    fn app_resource_reads_are_implicit_only_for_the_owning_server() {
        assert!(app_resource_uri_allowed("map", "map://sources"));
        assert!(app_resource_uri_allowed("map", "map://acquisition/acq-1"));
        assert!(app_resource_uri_allowed("map", "ui://map/admin.html"));
        assert!(!app_resource_uri_allowed("map", "map://"));
        assert!(!app_resource_uri_allowed("map", "timeseries://usage"));
        assert!(!app_resource_uri_allowed(
            "map",
            "ui://timeseries/forecast.html"
        ));
        assert!(!app_resource_uri_allowed("map", "map://../escape"));
        assert!(!app_resource_uri_allowed("time", "timeseries://usage"));
    }

    #[test]
    fn projected_dependency_admits_only_its_exact_resource_family() {
        let mut resource =
            veoveo_mcp_apps_extension::app_resource("ui://mission/operations.html", "operations");
        resource
            .meta
            .get_or_insert_with(rmcp::model::Meta::new)
            .0
            .insert(
                APP_RESOURCE_DEPENDENCIES_META_KEY.to_owned(),
                serde_json::to_value(vec![AppResourceDependency {
                    app_resource: veoveo_mcp_contract::ResourceUri::new(
                        "ui://mission/operations.html",
                    )
                    .unwrap(),
                    server: veoveo_mcp_contract::ServerSlug::new("view").unwrap(),
                    scheme: veoveo_mcp_contract::ResourceScheme::new("view").unwrap(),
                    uri_prefix: veoveo_mcp_contract::ResourceUriPrefix::new("view://frame/")
                        .unwrap(),
                    required_scope: veoveo_mcp_contract::ScopeName::new("view:read").unwrap(),
                    operations: std::collections::BTreeSet::from([AppResourceOperation::Read]),
                    data_labels: std::collections::BTreeSet::new(),
                }])
                .unwrap(),
            );
        assert!(app_dependency_allows_resource(
            &resource,
            "view://frame/frame-1"
        ));
        assert!(!app_dependency_allows_resource(&resource, "view://frames"));
        assert!(!app_dependency_allows_resource(
            &resource,
            "recording://frame/frame-1"
        ));
        assert!(!app_dependency_allows_resource(
            &resource,
            "view://frame/../views"
        ));
    }

    #[test]
    fn frame_csp_admits_only_exact_declared_origins() {
        let resource = veoveo_mcp_apps_extension::app_resource_with_meta(
            "ui://uav-sim/live.html",
            "uav-live",
            veoveo_mcp_apps_extension::ResourceUiMeta {
                csp: Some(veoveo_mcp_apps_extension::UiCsp {
                    connect_domains: vec![
                        "wss://stream.example.com".to_owned(),
                        "ws://127.0.0.1:49101".to_owned(),
                    ],
                    ..veoveo_mcp_apps_extension::UiCsp::default()
                }),
                prefers_border: Some(false),
            },
        );
        let policy = frame_csp(&resource).expect("valid CSP");
        let policy = policy.to_str().expect("ASCII CSP");
        assert!(policy.contains("connect-src ws://127.0.0.1:49101 wss://stream.example.com"));
        assert!(policy.contains("media-src blob:"));
        assert!(policy.contains("worker-src blob:"));

        let invalid = veoveo_mcp_apps_extension::app_resource_with_meta(
            "ui://uav-sim/live.html",
            "uav-live",
            veoveo_mcp_apps_extension::ResourceUiMeta {
                csp: Some(veoveo_mcp_apps_extension::UiCsp {
                    connect_domains: vec!["wss://stream.example.com/path".to_owned()],
                    ..veoveo_mcp_apps_extension::UiCsp::default()
                }),
                prefers_border: None,
            },
        );
        assert!(frame_csp(&invalid).is_err());
    }

    #[test]
    fn app_task_registry_admits_only_the_creating_view() {
        let registry = AppTaskRegistry::default();
        let now = Instant::now();
        registry.record_at(now, "task-1", "map", "ui://map/admin.html");
        assert!(registry.owns_at(now, "task-1", "map", "ui://map/admin.html"));
        assert!(!registry.owns_at(now, "task-1", "map", "ui://map/other.html"));
        assert!(!registry.owns_at(now, "task-1", "timeseries", "ui://map/admin.html"));
        assert!(!registry.owns_at(now, "task-2", "map", "ui://map/admin.html"));
    }

    #[test]
    fn app_task_registry_expires_entries_lazily() {
        let registry = AppTaskRegistry::default();
        let now = Instant::now();
        registry.record_at(now, "task-1", "map", "ui://map/admin.html");
        assert!(registry.owns_at(
            now + APP_TASK_RETENTION - Duration::from_secs(1),
            "task-1",
            "map",
            "ui://map/admin.html"
        ));
        assert!(!registry.owns_at(
            now + APP_TASK_RETENTION,
            "task-1",
            "map",
            "ui://map/admin.html"
        ));
        assert!(!registry.owns_at(now, "task-1", "map", "ui://map/admin.html"));
    }

    #[test]
    fn app_task_registry_evicts_the_oldest_entry_beyond_capacity() {
        let registry = AppTaskRegistry::default();
        let now = Instant::now();
        for index in 0..MAX_TRACKED_APP_TASKS {
            registry.record_at(now, &format!("task-{index}"), "map", "ui://map/admin.html");
        }
        assert!(registry.owns_at(now, "task-0", "map", "ui://map/admin.html"));
        registry.record_at(now, "task-overflow", "map", "ui://map/admin.html");
        assert!(!registry.owns_at(now, "task-0", "map", "ui://map/admin.html"));
        assert!(registry.owns_at(now, "task-1", "map", "ui://map/admin.html"));
        assert!(registry.owns_at(now, "task-overflow", "map", "ui://map/admin.html"));
    }

    #[test]
    fn app_task_registry_rerecords_a_task_id_without_duplicates() {
        let registry = AppTaskRegistry::default();
        let now = Instant::now();
        registry.record_at(now, "task-1", "map", "ui://map/admin.html");
        registry.record_at(now, "task-1", "map", "ui://map/other.html");
        assert!(!registry.owns_at(now, "task-1", "map", "ui://map/admin.html"));
        assert!(registry.owns_at(now, "task-1", "map", "ui://map/other.html"));
    }
}
