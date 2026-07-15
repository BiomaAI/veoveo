use std::fmt;

use rmcp::model::{
    CallToolResult, ContentBlock, ErrorData as McpError, ReadResourceResult, Resource,
    ResourceContents, ResourceTemplate, ServerResult, Tool,
};
use serde_json::Value;
use veoveo_mcp_contract::{
    GatewayAction, GatewayResourceProjection, GatewayToolName, McpMethodName, PolicyTarget,
    ResourceProjectionMode, ResourceUri, ServerManifest, ServerResourceUri, ServerSlug,
};

use crate::GatewayCatalog;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceReadKind {
    General,
    Artifact,
    Usage,
}

fn resource_read_kind(uri: &str) -> ResourceReadKind {
    match ServerResourceUri::parse(uri) {
        Ok(ServerResourceUri::Artifact { .. }) => ResourceReadKind::Artifact,
        Ok(ServerResourceUri::UsageRoot { .. } | ServerResourceUri::UsageTask { .. }) => {
            ResourceReadKind::Usage
        }
        Ok(_) | Err(_) => ResourceReadKind::General,
    }
}

pub(crate) fn resource_read_action(uri: &str) -> GatewayAction {
    match resource_read_kind(uri) {
        ResourceReadKind::Artifact => GatewayAction::ArtifactRead,
        ResourceReadKind::Usage => GatewayAction::UsageRead,
        ResourceReadKind::General => GatewayAction::ResourcesRead,
    }
}

pub(crate) fn gateway_resource_uri(uri: &str) -> Result<ResourceUri, McpError> {
    ResourceUri::new(uri.to_string())
        .map_err(|err| mcp_invalid_params(format!("invalid resource URI: {err}")))
}

fn split_resource_uri(uri: &str) -> Option<(&str, &str)> {
    uri.split_once("://")
}

fn projected_ui_resource_path(path: &str) -> &str {
    path.split_once('/').map(|(_, rest)| rest).unwrap_or(path)
}

pub(crate) fn project_upstream_resource_uri_for_gateway(
    server: &ServerManifest,
    upstream_uri: &str,
) -> Result<ResourceUri, McpError> {
    if server.resource_projection != ResourceProjectionMode::ServerOwned {
        return gateway_resource_uri(upstream_uri);
    }
    let (scheme, path) = split_resource_uri(upstream_uri)
        .ok_or_else(|| mcp_internal("upstream exposed invalid resource URI"))?;
    let gateway_uri = if scheme == "ui" {
        format!(
            "ui://{}/{}",
            server.slug.as_str(),
            projected_ui_resource_path(path)
        )
    } else if scheme == "http"
        || scheme == "https"
        || scheme == "data"
        || scheme == server.uri_scheme.as_str()
    {
        upstream_uri.to_string()
    } else {
        format!("{}://{path}", server.uri_scheme.as_str())
    };
    gateway_resource_uri(&gateway_uri)
}

pub(crate) fn project_gateway_resource_uri_for_upstream(
    server: &ServerManifest,
    gateway_uri: &str,
    upstream_resources: &[Resource],
) -> Result<Option<ResourceUri>, McpError> {
    if server.resource_projection != ResourceProjectionMode::ServerOwned {
        return Ok(Some(gateway_resource_uri(gateway_uri)?));
    }
    for resource in upstream_resources {
        let projected = project_upstream_resource_uri_for_gateway(server, &resource.uri)?;
        if projected.as_str() == gateway_uri {
            return Ok(Some(gateway_resource_uri(&resource.uri)?));
        }
    }
    Ok(None)
}

pub(crate) fn project_listed_resource_uri(
    server: &ServerManifest,
    resource: &mut Resource,
) -> Result<(), McpError> {
    resource.uri = project_upstream_resource_uri_for_gateway(server, &resource.uri)?.to_string();
    if let Some(meta) = &mut resource.meta {
        let mut value = Value::Object(meta.0.clone());
        project_meta_resource_uris(server, &mut value)?;
        if let Value::Object(projected) = value {
            meta.0 = projected;
        }
    }
    Ok(())
}

pub(crate) fn project_resource_template_uri(
    server: &ServerManifest,
    template: &mut ResourceTemplate,
) -> Result<(), McpError> {
    if let Some((scheme, path)) = split_resource_uri(&template.uri_template)
        && server.resource_projection == ResourceProjectionMode::ServerOwned
    {
        template.uri_template = if scheme == "ui" {
            format!(
                "ui://{}/{}",
                server.slug.as_str(),
                projected_ui_resource_path(path)
            )
        } else if scheme != server.uri_scheme.as_str()
            && scheme != "http"
            && scheme != "https"
            && scheme != "data"
        {
            format!("{}://{path}", server.uri_scheme.as_str())
        } else {
            template.uri_template.clone()
        };
    }
    if let Some(meta) = &mut template.meta {
        let mut value = Value::Object(meta.0.clone());
        project_meta_resource_uris(server, &mut value)?;
        if let Value::Object(projected) = value {
            meta.0 = projected;
        }
    }
    Ok(())
}

pub(crate) fn project_tool_resource_metadata(
    server: &ServerManifest,
    tool: &mut Tool,
) -> Result<(), McpError> {
    if let Some(meta) = &mut tool.meta {
        let mut value = Value::Object(meta.0.clone());
        project_meta_resource_uris(server, &mut value)?;
        if let Value::Object(projected) = value {
            meta.0 = projected;
        }
    }
    Ok(())
}

pub(crate) fn project_call_tool_resource_uris(
    server: &ServerManifest,
    result: &mut CallToolResult,
) -> Result<(), McpError> {
    if let Some(meta) = &mut result.meta {
        let mut value = Value::Object(meta.0.clone());
        project_meta_resource_uris(server, &mut value)?;
        if let Value::Object(projected) = value {
            meta.0 = projected;
        }
    }
    if let Some(structured) = &mut result.structured_content {
        project_meta_resource_uris(server, structured)?;
    }
    for content in &mut result.content {
        match content {
            ContentBlock::Resource(resource) => {
                project_resource_content_uri(server, &mut resource.resource)?;
                if let Some(meta) = &mut resource.meta {
                    let mut value = Value::Object(meta.0.clone());
                    project_meta_resource_uris(server, &mut value)?;
                    if let Value::Object(projected) = value {
                        meta.0 = projected;
                    }
                }
            }
            ContentBlock::ResourceLink(resource) => {
                project_listed_resource_uri(server, resource)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn project_resource_content_uri(
    server: &ServerManifest,
    content: &mut ResourceContents,
) -> Result<(), McpError> {
    match content {
        ResourceContents::TextResourceContents { uri, .. }
        | ResourceContents::BlobResourceContents { uri, .. } => {
            *uri = project_upstream_resource_uri_for_gateway(server, uri)?.to_string();
        }
        _ => {}
    }
    Ok(())
}

fn project_meta_resource_uris(server: &ServerManifest, value: &mut Value) -> Result<(), McpError> {
    match value {
        Value::String(text) if split_resource_uri(text).is_some() => {
            if let Ok(projected) = project_upstream_resource_uri_for_gateway(server, text) {
                *text = projected.to_string();
            }
        }
        Value::Array(values) => {
            for value in values {
                project_meta_resource_uris(server, value)?;
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                project_meta_resource_uris(server, value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn project_upstream_resource(
    server: &ServerManifest,
    uri: &str,
) -> Result<GatewayResourceProjection, McpError> {
    Ok(GatewayResourceProjection {
        server: server.slug.clone(),
        gateway_uri: project_upstream_resource_uri_for_gateway(server, uri)?,
        upstream_uri: gateway_resource_uri(uri)?,
    })
}

pub(crate) fn project_listed_resource(
    resource: &mut rmcp::model::Resource,
    projection: &GatewayResourceProjection,
) {
    resource.uri = projection.gateway_uri.to_string();
}

pub(crate) fn project_read_resource_result(
    result: &mut ReadResourceResult,
    projection: &GatewayResourceProjection,
) -> Result<(), McpError> {
    for content in &mut result.contents {
        match content {
            ResourceContents::TextResourceContents { uri, .. }
            | ResourceContents::BlobResourceContents { uri, .. }
                if uri == projection.upstream_uri.as_str() =>
            {
                *uri = projection.gateway_uri.to_string();
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn resource_policy_target(
    server: ServerSlug,
    uri: &str,
) -> Result<PolicyTarget, McpError> {
    let uri = gateway_resource_uri(uri)?;
    Ok(match resource_read_kind(uri.as_str()) {
        ResourceReadKind::Artifact => PolicyTarget::Artifact {
            server,
            artifact_uri: uri,
        },
        ResourceReadKind::Usage => PolicyTarget::Usage {
            server,
            usage_uri: uri,
        },
        ResourceReadKind::General => PolicyTarget::Resource { server, uri },
    })
}

pub(crate) fn audit_method_name(action: GatewayAction) -> Result<McpMethodName, McpError> {
    let method = match action {
        GatewayAction::ArtifactRead | GatewayAction::UsageRead => "resources/read",
        GatewayAction::AdminRead | GatewayAction::AdminWrite => {
            return Err(mcp_internal("admin audit method is not an MCP method"));
        }
        other => other
            .mcp_method()
            .ok_or_else(|| mcp_internal("gateway action does not map to an MCP method"))?,
    };
    McpMethodName::new(method).map_err(|err| mcp_internal(format!("invalid MCP method: {err}")))
}

pub(crate) fn parse_gateway_tool(
    catalog: &GatewayCatalog,
    name: &str,
) -> Result<crate::GatewayToolProjection, McpError> {
    let gateway_name = GatewayToolName::new(name.to_string())
        .map_err(|err| mcp_invalid_params(format!("invalid gateway tool name: {err}")))?;
    catalog
        .parse_tool_name(&gateway_name)
        .map_err(|err| mcp_invalid_params(err.to_string()))
}

pub(crate) fn ensure_unique_prompts(prompts: &[rmcp::model::Prompt]) -> Result<(), McpError> {
    let mut seen = std::collections::BTreeSet::<&str>::new();
    for prompt in prompts {
        if !seen.insert(prompt.name.as_str()) {
            return Err(mcp_internal(format!(
                "prompt `{}` is ambiguous across profile servers",
                prompt.name
            )));
        }
    }
    Ok(())
}

pub(crate) fn upstream_error(err: impl fmt::Display) -> McpError {
    mcp_internal(format!("upstream MCP request failed: {err}"))
}

pub(crate) fn unexpected_upstream_response(method: &str, response: ServerResult) -> McpError {
    mcp_internal(format!(
        "upstream returned unexpected response for {method}: {}",
        server_result_name(&response)
    ))
}

fn server_result_name(result: &ServerResult) -> &'static str {
    match result {
        ServerResult::InitializeResult(_) => "initialize",
        ServerResult::CompleteResult(_) => "complete",
        ServerResult::GetPromptResult(_) => "get_prompt",
        ServerResult::ListPromptsResult(_) => "list_prompts",
        ServerResult::ListResourcesResult(_) => "list_resources",
        ServerResult::ListResourceTemplatesResult(_) => "list_resource_templates",
        ServerResult::ReadResourceResult(_) => "read_resource",
        ServerResult::ListToolsResult(_) => "list_tools",
        ServerResult::ElicitResult(_) => "elicit",
        ServerResult::CreateTaskResult(_) => "create_task",
        ServerResult::ListTasksResult(_) => "list_tasks",
        ServerResult::GetTaskResult(_) => "get_task",
        ServerResult::CancelTaskResult(_) => "cancel_task",
        ServerResult::CallToolResult(_) => "call_tool",
        ServerResult::GetTaskPayloadResult(_) => "get_task_payload",
        ServerResult::EmptyResult(_) => "empty",
        ServerResult::CustomResult(_) => "custom",
    }
}

pub(crate) fn mcp_invalid_request(message: impl Into<String>) -> McpError {
    McpError::invalid_request(message.into(), None)
}

pub(crate) fn mcp_invalid_params(message: impl Into<String>) -> McpError {
    McpError::invalid_params(message.into(), None)
}

pub(crate) fn mcp_internal(message: impl Into<String>) -> McpError {
    McpError::internal_error(message.into(), None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::Resource;
    use veoveo_mcp_contract::{
        LocalToolName, McpSurfaceCapabilities, MountPath, ResourceScheme, ScopeName,
        UpstreamEndpoint, UpstreamTransport, UpstreamTransportSecurity, UpstreamUrl,
    };

    fn test_server(
        slug: &str,
        uri_scheme: &str,
        resource_projection: ResourceProjectionMode,
    ) -> ServerManifest {
        ServerManifest {
            slug: ServerSlug::new(slug).unwrap(),
            uri_scheme: ResourceScheme::new(uri_scheme).unwrap(),
            mount_path: MountPath::new(format!("/{slug}")).unwrap(),
            mcp_path: MountPath::new(format!("/{slug}/mcp")).unwrap(),
            upstream: UpstreamEndpoint {
                transport: UpstreamTransport::StreamableHttp,
                url: UpstreamUrl::new(format!("http://{slug}-mcp:8787/{slug}/mcp")).unwrap(),
                security: UpstreamTransportSecurity::ClusterInternalHttp,
                trusted_certificate_authorities: Vec::new(),
                client_certificate: None,
                client_private_key: None,
            },
            capabilities: McpSurfaceCapabilities {
                tools: true,
                resources: true,
                resource_templates: true,
                resource_subscriptions: false,
                prompts: true,
                completions: false,
                tasks: false,
                notifications: false,
            },
            resource_projection,
            tools: vec![LocalToolName::new("run").unwrap()],
            compatibility_helpers: Vec::new(),
            prompts: Vec::new(),
            required_scopes: vec![ScopeName::new("operator:use").unwrap()],
            owned_routes: Vec::new(),
            metadata: Value::Null,
        }
    }

    #[test]
    fn server_owned_projection_moves_vendor_resource_uris_under_server_namespace() {
        let server = test_server("charts", "charts", ResourceProjectionMode::ServerOwned);

        assert_eq!(
            project_upstream_resource_uri_for_gateway(&server, "vendor://chart-types")
                .unwrap()
                .as_str(),
            "charts://chart-types"
        );
        assert_eq!(
            project_upstream_resource_uri_for_gateway(&server, "ui://vendor/chart-view.html")
                .unwrap()
                .as_str(),
            "ui://charts/chart-view.html"
        );
    }

    #[test]
    fn server_owned_projection_roundtrips_listed_resources() {
        let server = test_server("charts", "charts", ResourceProjectionMode::ServerOwned);
        let upstream_resources = vec![
            Resource::new("vendor://chart-types", "chart-types"),
            Resource::new("ui://vendor/chart-view.html", "chart-view"),
        ];

        let upstream_uri = project_gateway_resource_uri_for_upstream(
            &server,
            "ui://charts/chart-view.html",
            &upstream_resources,
        )
        .unwrap()
        .unwrap();

        assert_eq!(upstream_uri.as_str(), "ui://vendor/chart-view.html");
    }

    #[test]
    fn server_owned_projection_updates_tool_result_structured_resource_uris() {
        let server = test_server("charts", "charts", ResourceProjectionMode::ServerOwned);
        let mut result = CallToolResult::success(vec![]);
        result.structured_content = Some(serde_json::json!({
            "resources": [
                "vendor://chart-types",
                { "resourceUri": "ui://vendor/chart-view.html" }
            ]
        }));

        project_call_tool_resource_uris(&server, &mut result).unwrap();

        let structured = result.structured_content.unwrap();
        assert_eq!(
            structured["resources"][0].as_str(),
            Some("charts://chart-types")
        );
        assert_eq!(
            structured["resources"][1]["resourceUri"].as_str(),
            Some("ui://charts/chart-view.html")
        );
    }
}
