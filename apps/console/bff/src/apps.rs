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

async fn apps_session(
    state: &AppState,
    request_headers: &HeaderMap,
) -> Result<McpSession, Response> {
    let session = api::upstream_session_for_apps(state, request_headers).await?;
    state
        .mcp
        .session(
            &state.config,
            &session.session.access_token,
            session.session.access_expires_at,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "console apps MCP session failed");
            StatusCode::BAD_GATEWAY.into_response()
        })
}

pub(crate) async fn list_apps(
    State(state): State<AppState>,
    request_headers: HeaderMap,
) -> Response {
    let mcp = match apps_session(&state, &request_headers).await {
        Ok(mcp) => mcp,
        Err(response) => return response,
    };
    let resources = match mcp.list_all_resources().await {
        Ok(resources) => resources,
        Err(error) => {
            tracing::error!(%error, "console apps resource listing failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let tools = match mcp.list_all_tools().await {
        Ok(tools) => tools,
        Err(error) => {
            tracing::error!(%error, "console apps tool listing failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
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
    let mcp = match apps_session(&state, &request_headers).await {
        Ok(mcp) => mcp,
        Err(response) => return response,
    };
    let result = match mcp
        .read_resource(rmcp::model::ReadResourceRequestParams::new(
            query.uri.as_str(),
        ))
        .await
    {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(%error, uri = %query.uri, "console app frame read failed");
            return StatusCode::NOT_FOUND.into_response();
        }
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
    let mcp = match apps_session(&state, &request_headers).await {
        Ok(mcp) => mcp,
        Err(response) => return response,
    };
    let gateway_tool = format!("{}__{}", request.server, request.tool);
    let tools = match mcp.list_all_tools().await {
        Ok(tools) => tools,
        Err(error) => {
            tracing::error!(%error, "console apps tool listing failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
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
}
