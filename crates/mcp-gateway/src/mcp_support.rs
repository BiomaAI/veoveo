use std::fmt;

use rmcp::model::{
    CallToolResult, ErrorData as McpError, GetTaskPayloadResult, ReadResourceResult,
    ResourceContents, ServerResult,
};
use veoveo_mcp_contract::{
    GatewayAction, GatewayProfileId, GatewayResourceProjection, GatewayTaskMapping,
    GatewayToolName, GenerationRunOutput, McpMethodName, PolicyTarget, PrincipalId, ResourceUri,
    ServerResourceUri, ServerSlug, TaskIdProjection, UpstreamTaskId, UsageReport,
    set_related_task_meta,
};

use crate::{GatewayCatalog, GatewayState};

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

pub(crate) fn task_mapping_allows_principal(
    profile_id: &GatewayProfileId,
    mapping: &GatewayTaskMapping,
    principal_id: &PrincipalId,
) -> bool {
    &mapping.profile == profile_id && &mapping.owner == principal_id
}

pub(crate) fn gateway_resource_uri(uri: &str) -> Result<ResourceUri, McpError> {
    ResourceUri::new(uri.to_string())
        .map_err(|err| mcp_invalid_params(format!("invalid resource URI: {err}")))
}

fn identity_resource_projection(
    server: ServerSlug,
    uri: &str,
) -> Result<GatewayResourceProjection, McpError> {
    let uri = gateway_resource_uri(uri)?;
    Ok(GatewayResourceProjection {
        server,
        gateway_uri: uri.clone(),
        upstream_uri: uri,
        task: None,
    })
}

pub(crate) fn project_upstream_resource_for_owner(
    state: &GatewayState,
    profile_id: &GatewayProfileId,
    owner: &PrincipalId,
    server: &ServerSlug,
    uri: &str,
) -> Result<Option<GatewayResourceProjection>, McpError> {
    let parsed = ServerResourceUri::parse(uri)
        .map_err(|err| mcp_internal(format!("upstream exposed invalid resource URI: {err}")))?;
    let Some(upstream_task_id) = parsed.usage_task_id() else {
        return Ok(Some(identity_resource_projection(server.clone(), uri)?));
    };
    let upstream_task_id = UpstreamTaskId::new(upstream_task_id.to_string())
        .map_err(|err| mcp_internal(format!("upstream exposed invalid usage task id: {err}")))?;
    let Some(mapping) = state
        .task_mapping_by_upstream(server, &upstream_task_id)
        .map_err(|err| mcp_internal(format!("failed to read gateway task mapping: {err}")))?
    else {
        return Ok(None);
    };
    if !task_mapping_allows_principal(profile_id, &mapping, owner) {
        return Ok(None);
    }
    let gateway_uri = parsed
        .with_usage_task_id(mapping.gateway_task_id.as_str())
        .map_err(|err| mcp_internal(format!("failed to project usage URI: {err}")))?
        .to_string();
    Ok(Some(GatewayResourceProjection {
        server: server.clone(),
        gateway_uri: gateway_resource_uri(&gateway_uri)?,
        upstream_uri: gateway_resource_uri(uri)?,
        task: Some(TaskIdProjection::from(&mapping)),
    }))
}

pub(crate) fn project_listed_resource(
    resource: &mut rmcp::model::Resource,
    projection: &GatewayResourceProjection,
) {
    resource.uri = projection.gateway_uri.to_string();
    if let Some(task) = &projection.task {
        resource.name = format!("usage for task {}", task.gateway_task_id);
        resource.description =
            Some("Usage estimates and actuals for one gateway task.".to_string());
    }
}

pub(crate) fn project_read_resource_result(
    result: &mut ReadResourceResult,
    projection: &GatewayResourceProjection,
) -> Result<(), McpError> {
    let Some(task) = &projection.task else {
        return Ok(());
    };
    for content in &mut result.contents {
        match content {
            ResourceContents::TextResourceContents { uri, text, .. } => {
                *uri = projection.gateway_uri.to_string();
                let mut report: UsageReport = serde_json::from_str(text).map_err(|err| {
                    mcp_internal(format!(
                        "upstream usage resource was not a usage report: {err}"
                    ))
                })?;
                project_usage_report(&mut report, projection, task);
                *text = serde_json::to_string(&report).map_err(|err| {
                    mcp_internal(format!("failed to encode projected usage report: {err}"))
                })?;
            }
            ResourceContents::BlobResourceContents { .. } => {
                return Err(mcp_internal(
                    "upstream usage resource returned blob content",
                ));
            }
            _ => {
                return Err(mcp_internal(
                    "upstream usage resource returned unknown content",
                ));
            }
        }
    }
    Ok(())
}

fn project_usage_report(
    report: &mut UsageReport,
    projection: &GatewayResourceProjection,
    task: &TaskIdProjection,
) {
    report.task_id = task.gateway_task_id.to_string();
    report.usage_uri = projection.gateway_uri.to_string();
    for record in &mut report.records {
        if record.task_id == task.upstream_task_id.as_str() {
            record.task_id = task.gateway_task_id.to_string();
        }
    }
}

pub(crate) fn project_task_payload_result(
    payload: &mut GetTaskPayloadResult,
    mapping: &GatewayTaskMapping,
) -> Result<(), McpError> {
    let mut result: CallToolResult = serde_json::from_value(payload.0.clone()).map_err(|err| {
        mcp_internal(format!(
            "upstream task payload was not a tool result: {err}"
        ))
    })?;
    project_call_tool_result(&mut result, mapping)?;
    payload.0 = serde_json::to_value(result)
        .map_err(|err| mcp_internal(format!("failed to encode projected task payload: {err}")))?;
    Ok(())
}

fn project_call_tool_result(
    result: &mut CallToolResult,
    mapping: &GatewayTaskMapping,
) -> Result<(), McpError> {
    set_related_task_meta(&mut result.meta, mapping.gateway_task_id.as_str());
    let Some(structured) = &mut result.structured_content else {
        return Ok(());
    };
    let Ok(mut output) = serde_json::from_value::<GenerationRunOutput>(structured.clone()) else {
        return Ok(());
    };
    for artifact in &mut output.artifacts {
        if let Some(metadata) = artifact.metadata.as_object_mut()
            && metadata.get("task_id").and_then(|value| value.as_str())
                == Some(mapping.upstream_task_id.as_str())
        {
            metadata.insert(
                "task_id".to_string(),
                serde_json::Value::String(mapping.gateway_task_id.to_string()),
            );
        }
    }
    *structured = serde_json::to_value(output).map_err(|err| {
        mcp_internal(format!(
            "failed to encode projected generation output: {err}"
        ))
    })?;
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
    use chrono::Utc;
    use rmcp::model::{GetTaskPayloadResult, ReadResourceResult, ResourceContents};
    use veoveo_mcp_contract::{GatewayTaskId, TaskIdProjection, UpstreamTaskId};

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = uuid::Uuid::new_v4();
        std::env::temp_dir().join(format!("veoveo-gateway-mcp-{name}-{unique}.duckdb"))
    }

    fn mapping(profile: &str, owner: &str) -> GatewayTaskMapping {
        let now = Utc::now();
        GatewayTaskMapping {
            gateway_task_id: GatewayTaskId::new("gateway-task-1").unwrap(),
            upstream_server: ServerSlug::new("media").unwrap(),
            upstream_task_id: UpstreamTaskId::new("upstream-task-1").unwrap(),
            profile: GatewayProfileId::new(profile).unwrap(),
            owner: PrincipalId::new(owner).unwrap(),
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn task_mapping_authorizes_only_owning_profile_and_principal() {
        let profile = GatewayProfileId::new("default").unwrap();
        let owner = PrincipalId::new("issuer#owner").unwrap();
        let mapping = mapping("default", "issuer#owner");

        assert!(task_mapping_allows_principal(&profile, &mapping, &owner));

        assert!(!task_mapping_allows_principal(
            &profile,
            &mapping,
            &PrincipalId::new("issuer#other").unwrap()
        ));
        assert!(!task_mapping_allows_principal(
            &GatewayProfileId::new("ops").unwrap(),
            &mapping,
            &owner
        ));
    }

    #[test]
    fn upstream_usage_resource_projects_to_gateway_task_id_for_owner() {
        let path = temp_path("usage-projection");
        let state = GatewayState::open(&path).unwrap();
        let mapping = mapping("default", "issuer#owner");
        state.record_task_mapping(&mapping).unwrap();

        let projection = project_upstream_resource_for_owner(
            &state,
            &GatewayProfileId::new("default").unwrap(),
            &PrincipalId::new("issuer#owner").unwrap(),
            &ServerSlug::new("media").unwrap(),
            "media://usage/task/upstream-task-1",
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            projection.gateway_uri.as_str(),
            "media://usage/task/gateway-task-1"
        );
        assert_eq!(
            projection.upstream_uri.as_str(),
            "media://usage/task/upstream-task-1"
        );
        assert_eq!(projection.task.unwrap(), TaskIdProjection::from(&mapping));

        assert!(
            project_upstream_resource_for_owner(
                &state,
                &GatewayProfileId::new("default").unwrap(),
                &PrincipalId::new("issuer#other").unwrap(),
                &ServerSlug::new("media").unwrap(),
                "media://usage/task/upstream-task-1",
            )
            .unwrap()
            .is_none()
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn usage_report_body_projects_to_gateway_task_id() {
        let mapping = mapping("default", "issuer#owner");
        let projection = GatewayResourceProjection {
            server: ServerSlug::new("media").unwrap(),
            gateway_uri: ResourceUri::new("media://usage/task/gateway-task-1").unwrap(),
            upstream_uri: ResourceUri::new("media://usage/task/upstream-task-1").unwrap(),
            task: Some(TaskIdProjection::from(&mapping)),
        };
        let text = serde_json::json!({
            "task_id": "upstream-task-1",
            "usage_uri": "media://usage/task/upstream-task-1",
            "records": [{
                "task_id": "upstream-task-1",
                "model_id": "fake/image",
                "kind": "actual",
                "amount": 0.01,
                "currency": "USD",
                "recorded_at": "2026-07-02T00:00:00Z",
                "metadata": null
            }]
        })
        .to_string();
        let mut result = ReadResourceResult::new(vec![ResourceContents::text(
            text,
            "media://usage/task/upstream-task-1",
        )]);

        project_read_resource_result(&mut result, &projection).unwrap();

        let ResourceContents::TextResourceContents { uri, text, .. } = &result.contents[0] else {
            panic!("expected text resource content");
        };
        assert_eq!(uri, "media://usage/task/gateway-task-1");
        let report: UsageReport = serde_json::from_str(text).unwrap();
        assert_eq!(report.task_id, "gateway-task-1");
        assert_eq!(report.usage_uri, "media://usage/task/gateway-task-1");
        assert_eq!(report.records[0].task_id, "gateway-task-1");
    }

    #[test]
    fn task_payload_generation_output_projects_artifact_task_metadata() {
        let mapping = mapping("default", "issuer#owner");
        let mut payload = GetTaskPayloadResult::new(serde_json::json!({
            "content": [],
            "structuredContent": {
                "prediction": {
                    "id": "prediction-1",
                    "model_id": "fake/image",
                    "status": "completed",
                    "output_count": 1
                },
                "artifacts": [{
                    "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "byte_len": 68,
                    "mime_type": "image/png",
                    "filename": "output.png",
                    "artifact_uri": "media://artifact/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "created_at": "2026-07-02T00:00:00Z",
                    "metadata": {
                        "task_id": "upstream-task-1",
                        "job_id": "prediction-1",
                        "model_id": "fake/image",
                        "output_index": 0
                    }
                }]
            }
        }));

        project_task_payload_result(&mut payload, &mapping).unwrap();

        let result: CallToolResult = serde_json::from_value(payload.0).unwrap();
        assert_eq!(
            result
                .meta
                .as_ref()
                .and_then(|meta| meta.0.get(veoveo_mcp_contract::RELATED_TASK_META_KEY))
                .and_then(|value| value.get("taskId"))
                .and_then(|value| value.as_str()),
            Some("gateway-task-1")
        );
        let output: GenerationRunOutput =
            serde_json::from_value(result.structured_content.unwrap()).unwrap();
        assert_eq!(
            output.artifacts[0].metadata["task_id"].as_str(),
            Some("gateway-task-1")
        );
    }
}
