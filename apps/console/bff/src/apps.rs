use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::CONTENT_TYPE},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use veoveo_mcp_apps_extension::{APP_MIME_TYPE, is_app_resource, tool_app_link};

use crate::{AppState, api, mcp_client::McpSession};

const MAX_APP_HTML_BYTES: usize = 2 * 1024 * 1024;
const MAX_CALL_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_CALL_RESULT_BYTES: usize = 2 * 1024 * 1024;

/// The frame document's CSP: no network, no storage, inline-only. Veoveo
/// apps are self-contained by contract; `_meta.ui.csp` connect domains are
/// deliberately not honored (deny-all host policy).
const FRAME_CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; \
     style-src 'unsafe-inline'; img-src data:; frame-ancestors 'self'";

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

/// Run `operation` against the pooled gateway MCP session, rebuilding the
/// session and retrying once when the transport is dead (e.g. the gateway
/// restarted and discarded every session). Returns the session actually
/// used so callers can issue follow-up calls without re-entering the pool.
async fn with_apps_session<T, F>(
    state: &AppState,
    request_headers: &HeaderMap,
    operation: impl Fn(McpSession) -> F,
) -> Result<(McpSession, Result<T, rmcp::ServiceError>), Response>
where
    F: Future<Output = Result<T, rmcp::ServiceError>>,
{
    let upstream = api::upstream_session_for_apps(state, request_headers).await?;
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
            result => return Ok((mcp, result)),
        }
    }
}

pub(crate) async fn list_apps(
    State(state): State<AppState>,
    request_headers: HeaderMap,
) -> Response {
    let listing = with_apps_session(&state, &request_headers, |mcp| async move {
        let resources = mcp.list_all_resources().await?;
        let tools = mcp.list_all_tools().await?;
        Ok((resources, tools))
    })
    .await;
    let (resources, tools) = match listing {
        Ok((_, Ok(listing))) => listing,
        Ok((_, Err(error))) => {
            tracing::error!(%error, "console apps listing failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
        Err(response) => return response,
    };
    let mut apps = Vec::new();
    for resource in resources
        .iter()
        .filter(|resource| is_app_resource(resource))
    {
        let Some(server) = app_uri_server(&resource.uri) else {
            continue;
        };
        let tools = tools
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
        });
    }
    Json(AppCatalog { apps }).into_response()
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
    let uri = query.uri.as_str();
    let read = with_apps_session(&state, &request_headers, |mcp| async move {
        mcp.read_resource(rmcp::model::ReadResourceRequestParams::new(uri))
            .await
    })
    .await;
    let result = match read {
        Ok((_, Ok(result))) => result,
        Ok((_, Err(error))) => {
            tracing::warn!(%error, uri = %query.uri, "console app frame read failed");
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(response) => return response,
    };
    let Some(html) = result.contents.iter().find_map(|contents| match contents {
        rmcp::model::ResourceContents::TextResourceContents {
            text, mime_type, ..
        } if mime_type.as_deref() == Some(APP_MIME_TYPE) => Some(text.clone()),
        _ => None,
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if html.len() > MAX_APP_HTML_BYTES {
        return StatusCode::BAD_GATEWAY.into_response();
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(FRAME_CSP),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("SAMEORIGIN"));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    (StatusCode::OK, headers, html).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadAppResourceRequest {
    server: String,
    app_uri: String,
    uri: String,
}

/// `resources/read` proxied for an app view. The allowlist mirrors
/// `call_app_tool`: the view must belong to the named server and may only
/// read that server's own resources.
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
    if !app_resource_uri_allowed(&request.server, &request.uri) {
        return call_error(
            StatusCode::FORBIDDEN,
            "resource is not owned by this app's server",
        );
    }
    let uri = request.uri.as_str();
    let read = with_apps_session(&state, &request_headers, |mcp| async move {
        mcp.read_resource(rmcp::model::ReadResourceRequestParams::new(uri))
            .await
    })
    .await;
    let result = match read {
        Ok((_, Ok(result))) => result,
        Ok((_, Err(error))) => {
            return call_error(
                StatusCode::BAD_GATEWAY,
                &format!("resource read failed: {error}"),
            );
        }
        Err(response) => return response,
    };
    let Ok(body) = serde_json::to_vec(&result) else {
        return StatusCode::BAD_GATEWAY.into_response();
    };
    if body.len() > MAX_CALL_RESULT_BYTES {
        return call_error(StatusCode::BAD_GATEWAY, "resource read exceeds the cap");
    }
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    (StatusCode::OK, headers, body).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CallAppToolRequest {
    server: String,
    app_uri: String,
    tool: String,
    #[serde(default)]
    arguments: serde_json::Value,
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
        mcp.list_all_tools().await
    })
    .await;
    // The tool call below deliberately stays single-shot on the session the
    // listing just proved healthy: tool calls are not idempotent, so only
    // rmcp's own in-transport replay may retry them.
    let (mcp, tools) = match listing {
        Ok((mcp, Ok(tools))) => (mcp, tools),
        Ok((_, Err(error))) => {
            tracing::error!(%error, "console apps tool listing failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
        Err(response) => return response,
    };
    let Some(tool) = tools.iter().find(|tool| tool.name.as_ref() == gateway_tool) else {
        return call_error(StatusCode::NOT_FOUND, "unknown tool for this app");
    };
    let allowed = tool_app_link(tool)
        .is_some_and(|link| link.visible_to_app() && link.resource_uri == request.app_uri);
    if !allowed {
        return call_error(
            StatusCode::FORBIDDEN,
            "tool is not app-visible for this view",
        );
    }
    let mut params = rmcp::model::CallToolRequestParams::new(gateway_tool);
    match request.arguments {
        serde_json::Value::Object(map) => {
            params = params.with_arguments(map.into_iter().collect());
        }
        serde_json::Value::Null => {}
        _ => return call_error(StatusCode::BAD_REQUEST, "arguments must be a JSON object"),
    }
    let result = match mcp.call_tool(params).await {
        Ok(result) => result,
        Err(error) => {
            return call_error(
                StatusCode::BAD_GATEWAY,
                &format!("tool call failed: {error}"),
            );
        }
    };
    let Ok(body) = serde_json::to_vec(&result) else {
        return StatusCode::BAD_GATEWAY.into_response();
    };
    if body.len() > MAX_CALL_RESULT_BYTES {
        return call_error(StatusCode::BAD_GATEWAY, "tool result exceeds the cap");
    }
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    (StatusCode::OK, headers, body).into_response()
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
    fn app_resource_reads_are_limited_to_the_owning_server() {
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
}
