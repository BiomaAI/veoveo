use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use chrono::{DateTime, TimeDelta, Utc};
use rmcp::{
    ClientHandler, ServiceExt,
    handler::server::ServerHandler,
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResult, CancelTaskParams,
        CancelTaskRequest, CancelTaskResult, ClientInfo, ClientRequest, CompleteRequestParams,
        CompleteResult, CreateTaskResult, ErrorData as McpError, ExtensionCapabilities,
        GetPromptRequestParams, GetPromptResult, GetTaskParams, GetTaskPayloadParams,
        GetTaskPayloadRequest, GetTaskPayloadResult, GetTaskRequest, GetTaskResult, Implementation,
        InitializeRequestParams, InitializeResult, JsonObject, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListTasksResult, ListToolsResult,
        Notification, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        Reference, ResourceContents, ServerCapabilities, ServerInfo, ServerNotification,
        ServerResult, SubscribeRequestParams, TaskStatusNotificationParam, TasksCapability,
        UnsubscribeRequestParams,
    },
    service::{NotificationContext, Peer, RequestContext, RoleClient, RoleServer, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tokio::sync::RwLock;
use veoveo_mcp_contract::{
    AuditEvent, CompletionExposure, GatewayAction, GatewayInternalTokenIssuer, GatewayProfileId,
    GatewayResourceProjection, GatewayResourceSubscription, GatewayTaskId, GatewayTaskMapping,
    GatewayToolName, GenerationRunOutput, LocalToolName, McpMethodName, PolicyDecision,
    PolicyEffect, PolicyReasonCode, PolicyTarget, PrincipalId, PromptName, ResourceUri,
    ServerResourceUri, ServerSlug, TaskExposure as ContractTaskExposure, TaskIdProjection, TraceId,
    UpstreamTaskId, UpstreamTransport, UsageReport, paginate,
};

use crate::{
    AuthenticatedSubject, GatewayCatalog, GatewayCatalogHandle, GatewayState, PolicyRequest,
};

const GATEWAY_PAGE_SIZE: usize = 100;
const INTERNAL_TOKEN_TTL_SECONDS: i64 = 15 * 60;
const INTERNAL_TOKEN_REFRESH_WINDOW_SECONDS: i64 = 30;

#[derive(Debug)]
pub struct GatewayMcp {
    catalog: GatewayCatalogHandle,
    state: GatewayState,
    profile_id: GatewayProfileId,
    internal_token_issuer: GatewayInternalTokenIssuer,
    upstreams: RwLock<BTreeMap<UpstreamCacheKey, UpstreamConnection>>,
}

impl GatewayMcp {
    pub fn new(
        catalog: GatewayCatalogHandle,
        profile_id: GatewayProfileId,
        state: GatewayState,
        internal_token_issuer: GatewayInternalTokenIssuer,
    ) -> Self {
        Self {
            catalog,
            state,
            profile_id,
            internal_token_issuer,
            upstreams: RwLock::new(BTreeMap::new()),
        }
    }

    fn profile_servers(&self) -> Vec<ServerSlug> {
        self.catalog
            .current()
            .profile_servers(&self.profile_id)
            .into_iter()
            .map(|(_, server)| server.slug.clone())
            .collect()
    }

    fn profile_task_servers(&self) -> Vec<ServerSlug> {
        self.catalog
            .current()
            .profile_servers(&self.profile_id)
            .into_iter()
            .filter(|(exposure, server)| {
                exposure.tasks == ContractTaskExposure::Enabled && server.capabilities.tasks
            })
            .map(|(_, server)| server.slug.clone())
            .collect()
    }

    fn auth_extension_capabilities(&self) -> Option<ExtensionCapabilities> {
        let catalog = self.catalog.current();
        let profile = catalog.profile(&self.profile_id)?;
        let extensions: ExtensionCapabilities = profile
            .auth_modes
            .iter()
            .filter_map(|mode| mode.mcp_extension_id())
            .map(|extension| (extension.to_string(), JsonObject::new()))
            .collect();
        if extensions.is_empty() {
            None
        } else {
            Some(extensions)
        }
    }

    async fn upstream(
        &self,
        server_slug: &ServerSlug,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<Peer<RoleClient>, McpError> {
        let snapshot = self.catalog.snapshot();
        let catalog_generation = snapshot.generation();
        let key = UpstreamCacheKey {
            server: server_slug.clone(),
            principal: subject.principal.id.clone(),
            catalog_generation,
        };
        let refresh_after = Utc::now() + TimeDelta::seconds(INTERNAL_TOKEN_REFRESH_WINDOW_SECONDS);
        {
            let upstreams = self.upstreams.read().await;
            if let Some(connection) = upstreams.get(&key)
                && !connection.running.is_closed()
                && connection.expires_at > refresh_after
            {
                return Ok(connection.running.peer().clone());
            }
        }

        let server = snapshot
            .catalog()
            .server(server_slug)
            .ok_or_else(|| mcp_invalid_params(format!("unknown upstream server `{server_slug}`")))?
            .clone();
        if server.upstream.transport != UpstreamTransport::StreamableHttp {
            return Err(mcp_internal(format!(
                "unsupported upstream transport for server `{server_slug}`"
            )));
        }

        let mut upstreams = self.upstreams.write().await;
        if let Some(connection) = upstreams.get(&key)
            && !connection.running.is_closed()
            && connection.expires_at > refresh_after
        {
            return Ok(connection.running.peer().clone());
        }
        upstreams.remove(&key);

        let token_expires_at = internal_token_expires_at(subject)?;
        let internal_token = self
            .internal_token_issuer
            .issue(
                self.profile_id.clone(),
                server_slug.clone(),
                subject.principal.clone(),
                token_expires_at,
            )
            .map_err(|err| mcp_internal(format!("failed to issue internal token: {err}")))?;

        let transport = StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(server.upstream.url.as_str().to_string())
                .auth_header(internal_token.bearer_token)
                .reinit_on_expired_session(false),
        );
        let handler = GatewayUpstreamHandler {
            profile_id: self.profile_id.clone(),
            principal_id: subject.principal.id.clone(),
            upstream_server: server_slug.clone(),
            state: self.state.clone(),
            downstream,
        };
        let running = handler
            .serve(transport)
            .await
            .map_err(|err| mcp_internal(format!("failed to initialize upstream MCP: {err}")))?;
        let peer = running.peer().clone();
        upstreams.insert(
            key,
            UpstreamConnection {
                running,
                expires_at: internal_token.identity.expires_at,
            },
        );
        Ok(peer)
    }

    fn authenticated(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<AuthenticatedSubject, McpError> {
        let parts = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| mcp_invalid_request("authenticated HTTP context missing"))?;
        parts
            .extensions
            .get::<AuthenticatedSubject>()
            .cloned()
            .ok_or_else(|| mcp_invalid_request("authenticated subject missing"))
    }

    fn authorize(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<AuthenticatedSubject, McpError> {
        let (subject, decision) = self.evaluate_policy(context, action, target)?;
        if decision.effect == PolicyEffect::Allow {
            Ok(subject)
        } else {
            tracing::warn!(
                profile = %self.profile_id,
                principal = %subject.principal.id,
                action = ?action,
                reason = ?decision.reason,
                "gateway policy denied MCP request"
            );
            Err(mcp_invalid_request(format!(
                "gateway policy denied request: {:?}",
                decision.reason
            )))
        }
    }

    fn allows(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<bool, McpError> {
        let (_subject, decision) = self.evaluate_policy(context, action, target)?;
        Ok(decision.effect == PolicyEffect::Allow)
    }

    fn evaluate_policy(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<(AuthenticatedSubject, PolicyDecision), McpError> {
        let subject = self.authenticated(context)?;
        let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())
            .map_err(|err| mcp_internal(format!("failed to create trace id: {err}")))?;
        let catalog = self.catalog.current();
        let decision = catalog.decide(PolicyRequest {
            principal: &subject.principal,
            profile: &self.profile_id,
            action,
            target: &target,
            trace_id: &trace_id,
        });
        let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())
            .map_err(|err| mcp_internal(format!("failed to create audit event id: {err}")))?;
        self.state
            .record_audit_event(&AuditEvent {
                event_id,
                timestamp: decision.evaluated_at,
                trace_id,
                profile: self.profile_id.clone(),
                method: audit_method_name(action)?,
                action,
                target,
                decision: decision.clone(),
                principal: Some(subject.principal.id.clone()),
                tenant: subject.principal.tenant.clone(),
                token_issuer: Some(subject.access_token.issuer.clone()),
                latency_ms: None,
                metadata: BTreeMap::new(),
            })
            .map_err(|err| mcp_internal(format!("failed to record gateway audit event: {err}")))?;
        Ok((subject, decision))
    }

    fn authorize_tool(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        tool: LocalToolName,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize(context, action, PolicyTarget::Tool { server, tool })
    }

    fn allows_tool(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        tool: LocalToolName,
    ) -> Result<bool, McpError> {
        self.allows(context, action, PolicyTarget::Tool { server, tool })
    }

    fn authorize_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        uri: &str,
    ) -> Result<AuthenticatedSubject, McpError> {
        let target = resource_policy_target(server, uri)?;
        self.authorize(context, action, target)
    }

    fn authorize_projected_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        projection: &GatewayResourceProjection,
    ) -> Result<AuthenticatedSubject, McpError> {
        let Some(task) = &projection.task else {
            return self.authorize_resource(
                context,
                action,
                projection.server.clone(),
                projection.gateway_uri.as_str(),
            );
        };
        let subject = self.authenticated(context)?;
        let target =
            resource_policy_target(projection.server.clone(), projection.gateway_uri.as_str())?;
        if task.upstream_server != projection.server {
            return Err(mcp_internal("invalid gateway resource projection"));
        }
        let mapping = self.task_mapping(task.gateway_task_id.as_str())?;
        if mapping.upstream_server != projection.server
            || mapping.upstream_task_id != task.upstream_task_id
            || !task_mapping_allows_principal(&self.profile_id, &mapping, &subject.principal.id)
        {
            self.record_policy_denial(&subject, action, target, PolicyReasonCode::UnknownTask)?;
            return Err(mcp_invalid_params("unknown gateway task id"));
        }
        self.authorize(context, action, target)
    }

    fn allows_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        uri: &str,
    ) -> Result<bool, McpError> {
        let target = resource_policy_target(server, uri)?;
        self.allows(context, action, target)
    }

    fn authorize_prompt(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        prompt: PromptName,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize(context, action, PolicyTarget::Prompt { server, prompt })
    }

    fn allows_prompt(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        prompt: PromptName,
    ) -> Result<bool, McpError> {
        self.allows(context, action, PolicyTarget::Prompt { server, prompt })
    }

    fn authorize_mapped_task(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        mapping: &GatewayTaskMapping,
    ) -> Result<AuthenticatedSubject, McpError> {
        let subject = self.authenticated(context)?;
        let target = PolicyTarget::Task {
            server: mapping.upstream_server.clone(),
            gateway_task_id: mapping.gateway_task_id.clone(),
        };
        if !task_mapping_allows_principal(&self.profile_id, mapping, &subject.principal.id) {
            self.record_policy_denial(&subject, action, target, PolicyReasonCode::UnknownTask)?;
            return Err(mcp_invalid_params("unknown gateway task id"));
        }
        self.authorize(context, action, target)
    }

    fn record_policy_denial(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: PolicyTarget,
        reason: PolicyReasonCode,
    ) -> Result<(), McpError> {
        let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())
            .map_err(|err| mcp_internal(format!("failed to create trace id: {err}")))?;
        let event_id = TraceId::new(uuid::Uuid::new_v4().to_string())
            .map_err(|err| mcp_internal(format!("failed to create audit event id: {err}")))?;
        let policy_version = self
            .catalog
            .current()
            .profile(&self.profile_id)
            .map(|profile| profile.policy_version.clone());
        let decision = PolicyDecision {
            effect: PolicyEffect::Deny,
            reason,
            evaluated_at: Utc::now(),
            profile: self.profile_id.clone(),
            action,
            target: target.clone(),
            principal: Some(subject.principal.id.clone()),
            tenant: subject.principal.tenant.clone(),
            policy_version,
            rule_id: None,
            trace_id: trace_id.clone(),
        };
        self.state
            .record_audit_event(&AuditEvent {
                event_id,
                timestamp: decision.evaluated_at,
                trace_id,
                profile: self.profile_id.clone(),
                method: audit_method_name(action)?,
                action,
                target,
                decision,
                principal: Some(subject.principal.id.clone()),
                tenant: subject.principal.tenant.clone(),
                token_issuer: Some(subject.access_token.issuer.clone()),
                latency_ms: None,
                metadata: BTreeMap::new(),
            })
            .map_err(|err| mcp_internal(format!("failed to record gateway audit event: {err}")))?;
        Ok(())
    }

    fn task_mapping(&self, task_id: &str) -> Result<GatewayTaskMapping, McpError> {
        let gateway_task_id = GatewayTaskId::new(task_id.to_string())
            .map_err(|err| mcp_invalid_params(format!("invalid gateway task id: {err}")))?;
        self.state
            .task_mapping(&gateway_task_id)
            .map_err(|err| mcp_internal(format!("failed to read gateway task mapping: {err}")))?
            .ok_or_else(|| mcp_invalid_params("unknown gateway task id"))
    }

    fn server_for_resource(&self, uri: &str) -> Result<ServerSlug, McpError> {
        self.catalog
            .current()
            .server_for_resource_uri(&self.profile_id, uri)
            .map(|(_, server)| server.slug.clone())
            .ok_or_else(|| mcp_invalid_params(format!("resource URI is not exposed: {uri}")))
    }

    fn project_resource_for_upstream(
        &self,
        uri: &str,
    ) -> Result<GatewayResourceProjection, McpError> {
        let server = self.server_for_resource(uri)?;
        let parsed = ServerResourceUri::parse(uri)
            .map_err(|err| mcp_invalid_params(format!("invalid resource URI: {err}")))?;
        let Some(task_id) = parsed.usage_task_id() else {
            return Ok(GatewayResourceProjection {
                server,
                gateway_uri: gateway_resource_uri(uri)?,
                upstream_uri: gateway_resource_uri(uri)?,
                task: None,
            });
        };
        let mapping = self.task_mapping(task_id)?;
        if mapping.upstream_server != server {
            return Err(mcp_invalid_params(
                "usage task id belongs to another server",
            ));
        }
        let upstream_uri = parsed
            .with_usage_task_id(mapping.upstream_task_id.as_str())
            .map_err(|err| mcp_internal(format!("failed to project usage URI: {err}")))?
            .to_string();
        Ok(GatewayResourceProjection {
            server,
            gateway_uri: gateway_resource_uri(uri)?,
            upstream_uri: gateway_resource_uri(&upstream_uri)?,
            task: Some(TaskIdProjection::from(&mapping)),
        })
    }

    fn project_upstream_resource_for_owner(
        &self,
        server: &ServerSlug,
        owner: &PrincipalId,
        uri: &str,
    ) -> Result<Option<GatewayResourceProjection>, McpError> {
        project_upstream_resource_for_owner(&self.state, &self.profile_id, owner, server, uri)
    }

    fn server_for_prompt(&self, prompt: &str) -> Result<ServerSlug, McpError> {
        let prompt = PromptName::new(prompt.to_string())
            .map_err(|err| mcp_invalid_params(format!("invalid prompt name: {err}")))?;
        let catalog = self.catalog.current();
        let matches = catalog.prompt_servers(&self.profile_id, &prompt);
        match matches.as_slice() {
            [(_, server)] => Ok(server.slug.clone()),
            [] => Err(mcp_invalid_params(format!(
                "prompt is not exposed: {prompt}"
            ))),
            _ => Err(mcp_internal(format!(
                "prompt `{prompt}` is ambiguous across profile servers"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct UpstreamCacheKey {
    server: ServerSlug,
    principal: PrincipalId,
    catalog_generation: u64,
}

#[derive(Debug)]
struct UpstreamConnection {
    running: RunningService<RoleClient, GatewayUpstreamHandler>,
    expires_at: DateTime<Utc>,
}

fn internal_token_expires_at(subject: &AuthenticatedSubject) -> Result<DateTime<Utc>, McpError> {
    let now = Utc::now();
    let max_expires_at = now + TimeDelta::seconds(INTERNAL_TOKEN_TTL_SECONDS);
    let expires_at = std::cmp::min(subject.access_token.expires_at, max_expires_at);
    if expires_at <= now {
        return Err(mcp_invalid_request(
            "authenticated access token is already expired",
        ));
    }
    Ok(expires_at)
}

fn task_mapping_allows_principal(
    profile_id: &GatewayProfileId,
    mapping: &GatewayTaskMapping,
    principal_id: &PrincipalId,
) -> bool {
    &mapping.profile == profile_id && &mapping.owner == principal_id
}

fn gateway_resource_uri(uri: &str) -> Result<ResourceUri, McpError> {
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

fn project_upstream_resource_for_owner(
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

fn project_listed_resource(
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

fn project_read_resource_result(
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

fn project_task_payload_result(
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

fn resource_policy_target(server: ServerSlug, uri: &str) -> Result<PolicyTarget, McpError> {
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

fn audit_method_name(action: GatewayAction) -> Result<McpMethodName, McpError> {
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

impl ServerHandler for GatewayMcp {
    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::default();
        let catalog = self.catalog.current();
        for (_, server) in catalog.profile_servers(&self.profile_id) {
            if server.capabilities.tools {
                capabilities.tools.get_or_insert_default();
            }
            if server.capabilities.prompts {
                capabilities.prompts.get_or_insert_default();
            }
            if server.capabilities.resources || server.capabilities.resource_templates {
                let resources = capabilities.resources.get_or_insert_default();
                if server.capabilities.resource_subscriptions {
                    resources.subscribe = Some(true);
                }
                if server.capabilities.notifications {
                    resources.list_changed = Some(true);
                }
            }
            if server.capabilities.completions {
                capabilities.completions.get_or_insert_with(JsonObject::new);
            }
            if server.capabilities.tasks {
                capabilities
                    .tasks
                    .get_or_insert_with(TasksCapability::server_default);
            }
        }
        capabilities.extensions = self.auth_extension_capabilities();

        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = Implementation::new("veoveo-gateway", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Gateway profile for hosted Veoveo MCP servers. Tool names are gateway namespaced; resource URIs remain server-owned."
                .to_string(),
        );
        info
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        self.authenticated(&context)?;
        context.peer.set_peer_info(request);
        Ok(self.get_info())
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let subject = self.authenticated(&context)?;
        let mut tools = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for mut tool in upstream.list_all_tools().await.map_err(upstream_error)? {
                let local_tool =
                    LocalToolName::new(tool.name.as_ref().to_string()).map_err(|err| {
                        mcp_internal(format!("upstream exposed invalid tool name: {err}"))
                    })?;
                if !self.allows_tool(
                    &context,
                    GatewayAction::ToolsList,
                    server_slug.clone(),
                    local_tool.clone(),
                )? {
                    continue;
                }
                let gateway_name = self
                    .catalog
                    .current()
                    .project_tool_name(&server_slug, &local_tool)
                    .map_err(|err| mcp_internal(format!("failed to project tool name: {err}")))?;
                tool.name = Cow::Owned(gateway_name.to_string());
                tools.push(tool);
            }
        }
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let page = paginate(tools, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let catalog = self.catalog.current();
        let projection = parse_gateway_tool(&catalog, &request.name)?;
        let subject = self.authorize_tool(
            &context,
            GatewayAction::ToolsCall,
            projection.server.clone(),
            projection.tool.clone(),
        )?;
        request.name = Cow::Owned(projection.tool.to_string());
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        upstream.call_tool(request).await.map_err(upstream_error)
    }

    async fn enqueue_task(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        let catalog = self.catalog.current();
        let projection = parse_gateway_tool(&catalog, &request.name)?;
        let subject = self.authorize_tool(
            &context,
            GatewayAction::ToolsCall,
            projection.server.clone(),
            projection.tool.clone(),
        )?;
        request.name = Cow::Owned(projection.tool.to_string());
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::CallToolRequest(CallToolRequest::new(
                request,
            )))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::CreateTaskResult(mut result) => {
                let upstream_task_id =
                    UpstreamTaskId::new(result.task.task_id.clone()).map_err(|err| {
                        mcp_internal(format!("upstream returned invalid task id: {err}"))
                    })?;
                let gateway_task_id = GatewayTaskId::new(uuid::Uuid::new_v4().to_string())
                    .map_err(|err| {
                        mcp_internal(format!("failed to create gateway task id: {err}"))
                    })?;
                let now = chrono::Utc::now();
                self.state
                    .record_task_mapping(&GatewayTaskMapping {
                        gateway_task_id: gateway_task_id.clone(),
                        upstream_server: projection.server.clone(),
                        upstream_task_id,
                        profile: self.profile_id.clone(),
                        owner: subject.principal.id.clone(),
                        created_at: now,
                        updated_at: now,
                    })
                    .map_err(|err| {
                        mcp_internal(format!("failed to persist gateway task mapping: {err}"))
                    })?;
                result.task.task_id = gateway_task_id.to_string();
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tools/call task", other)),
        }
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let subject = self.authenticated(&context)?;
        let mut resources = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for mut resource in upstream
                .list_all_resources()
                .await
                .map_err(upstream_error)?
            {
                let Some(projection) = self.project_upstream_resource_for_owner(
                    &server_slug,
                    &subject.principal.id,
                    &resource.uri,
                )?
                else {
                    continue;
                };
                project_listed_resource(&mut resource, &projection);
                if !self.allows_resource(
                    &context,
                    GatewayAction::ResourcesList,
                    projection.server.clone(),
                    &resource.uri,
                )? {
                    continue;
                }
                resources.push(resource);
            }
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        let page = paginate(resources, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let subject = self.authenticated(&context)?;
        let mut templates = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for template in upstream
                .list_all_resource_templates()
                .await
                .map_err(upstream_error)?
            {
                if !self.allows_resource(
                    &context,
                    GatewayAction::ResourcesTemplatesList,
                    server_slug.clone(),
                    &template.uri_template,
                )? {
                    continue;
                }
                templates.push(template);
            }
        }
        templates.sort_by(|left, right| left.uri_template.cmp(&right.uri_template));
        let page = paginate(templates, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        mut request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let projection = self.project_resource_for_upstream(&request.uri)?;
        let subject = self.authorize_projected_resource(
            &context,
            resource_read_action(&request.uri),
            &projection,
        )?;
        request.uri = projection.upstream_uri.to_string();
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        let mut result = upstream
            .read_resource(request)
            .await
            .map_err(upstream_error)?;
        project_read_resource_result(&mut result, &projection)?;
        Ok(result)
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let mut request = request;
        let uri = request.uri.clone();
        let projection = self.project_resource_for_upstream(&uri)?;
        let resource_uri = projection.gateway_uri.clone();
        let subject = self.authorize_projected_resource(
            &context,
            GatewayAction::ResourcesSubscribe,
            &projection,
        )?;
        request.uri = projection.upstream_uri.to_string();
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        upstream.subscribe(request).await.map_err(upstream_error)?;
        let now = Utc::now();
        self.state
            .record_resource_subscription(&GatewayResourceSubscription {
                profile: self.profile_id.clone(),
                owner: subject.principal.id,
                upstream_server: projection.server,
                resource_uri,
                created_at: now,
                updated_at: now,
            })
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to persist gateway resource subscription: {err}"
                ))
            })?;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let mut request = request;
        let uri = request.uri.clone();
        let projection = self.project_resource_for_upstream(&uri)?;
        let resource_uri = projection.gateway_uri.clone();
        let server = projection.server.clone();
        let subject = self.authenticated(&context)?;
        let subscription = self
            .state
            .resource_subscription(
                &self.profile_id,
                &subject.principal.id,
                &server,
                &resource_uri,
            )
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to read gateway resource subscription: {err}"
                ))
            })?;
        if subscription.is_none() {
            self.record_policy_denial(
                &subject,
                GatewayAction::ResourcesUnsubscribe,
                resource_policy_target(server.clone(), resource_uri.as_str())?,
                PolicyReasonCode::UnknownResource,
            )?;
            return Err(mcp_invalid_params("unknown gateway resource subscription"));
        }
        let subject = self.authorize_projected_resource(
            &context,
            GatewayAction::ResourcesUnsubscribe,
            &projection,
        )?;
        request.uri = projection.upstream_uri.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        upstream
            .unsubscribe(request)
            .await
            .map_err(upstream_error)?;
        self.state
            .delete_resource_subscription(
                &self.profile_id,
                &subject.principal.id,
                &server,
                &resource_uri,
            )
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to delete gateway resource subscription: {err}"
                ))
            })?;
        Ok(())
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let subject = self.authenticated(&context)?;
        let mut prompts = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for prompt in upstream.list_all_prompts().await.map_err(upstream_error)? {
                let prompt_name = PromptName::new(prompt.name.clone()).map_err(|err| {
                    mcp_internal(format!("upstream exposed invalid prompt name: {err}"))
                })?;
                if !self.allows_prompt(
                    &context,
                    GatewayAction::PromptsList,
                    server_slug.clone(),
                    prompt_name,
                )? {
                    continue;
                }
                prompts.push(prompt);
            }
        }
        ensure_unique_prompts(&prompts)?;
        prompts.sort_by(|left, right| left.name.cmp(&right.name));
        let page = paginate(prompts, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListPromptsResult {
            prompts: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let server = self.server_for_prompt(&request.name)?;
        let prompt = PromptName::new(request.name.clone())
            .map_err(|err| mcp_invalid_params(format!("invalid prompt name: {err}")))?;
        let subject =
            self.authorize_prompt(&context, GatewayAction::PromptsGet, server.clone(), prompt)?;
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        upstream.get_prompt(request).await.map_err(upstream_error)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let server = match &request.r#ref {
            Reference::Resource(reference) => self.server_for_resource(&reference.uri)?,
            Reference::Prompt(reference) => self.server_for_prompt(&reference.name)?,
            _ => return Err(mcp_invalid_params("unsupported completion reference kind")),
        };
        let catalog = self.catalog.current();
        let (_profile, exposure, manifest) = catalog
            .profile_server(&self.profile_id, &server)
            .ok_or_else(|| mcp_invalid_params(format!("server `{server}` is not exposed")))?;
        if exposure.completions != CompletionExposure::Enabled || !manifest.capabilities.completions
        {
            return Err(mcp_invalid_request("profile does not expose completions"));
        }
        let target = match &request.r#ref {
            Reference::Resource(reference) => {
                let uri = ResourceUri::new(reference.uri.clone())
                    .map_err(|err| mcp_invalid_params(format!("invalid completion URI: {err}")))?;
                PolicyTarget::Resource {
                    server: server.clone(),
                    uri,
                }
            }
            Reference::Prompt(reference) => {
                let prompt = PromptName::new(reference.name.clone()).map_err(|err| {
                    mcp_invalid_params(format!("invalid completion prompt: {err}"))
                })?;
                PolicyTarget::Prompt {
                    server: server.clone(),
                    prompt,
                }
            }
            _ => return Err(mcp_invalid_params("unsupported completion reference kind")),
        };
        let subject = self.authorize(&context, GatewayAction::CompletionComplete, target)?;
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        upstream.complete(request).await.map_err(upstream_error)
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let subject = self.authenticated(&context)?;
        let task_servers = self
            .profile_task_servers()
            .into_iter()
            .collect::<BTreeSet<_>>();
        if task_servers.is_empty() {
            return Err(mcp_invalid_request("profile does not expose MCP tasks"));
        }
        let mut allowed_task_servers = BTreeSet::new();
        for server in task_servers {
            let allowed = self.allows(
                &context,
                GatewayAction::TasksList,
                PolicyTarget::TaskList {
                    server: server.clone(),
                },
            )?;
            if allowed {
                allowed_task_servers.insert(server);
            }
        }

        let all_mappings = self
            .state
            .task_mappings_for_profile_owner(&self.profile_id, &subject.principal.id)
            .map_err(|err| mcp_internal(format!("failed to read gateway task mappings: {err}")))?;
        let mut mappings = Vec::new();
        for mapping in all_mappings {
            if !allowed_task_servers.contains(&mapping.upstream_server) {
                continue;
            }
            mappings.push(mapping);
        }

        let page = paginate(mappings, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        let mut tasks = Vec::with_capacity(page.items.len());
        for mapping in page.items {
            let server = mapping.upstream_server.clone();
            let upstream = self
                .upstream(&server, context.peer.clone(), &subject)
                .await?;
            let result = upstream
                .send_request(ClientRequest::GetTaskRequest(GetTaskRequest::new(
                    GetTaskParams::new(mapping.upstream_task_id.to_string()),
                )))
                .await
                .map_err(upstream_error)?;
            match result {
                ServerResult::GetTaskResult(mut result) => {
                    result.task.task_id = mapping.gateway_task_id.to_string();
                    tasks.push(result.task);
                }
                other => return Err(unexpected_upstream_response("tasks/get", other)),
            }
        }

        let mut result = ListTasksResult::new(tasks);
        result.next_cursor = page.next_cursor;
        Ok(result)
    }

    async fn get_task_info(
        &self,
        mut request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let mapping = self.task_mapping(&request.task_id)?;
        let server = mapping.upstream_server.clone();
        let subject = self.authorize_mapped_task(&context, GatewayAction::TasksGet, &mapping)?;
        request.task_id = mapping.upstream_task_id.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::GetTaskRequest(GetTaskRequest::new(request)))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::GetTaskResult(mut result) => {
                result.task.task_id = mapping.gateway_task_id.to_string();
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tasks/get", other)),
        }
    }

    async fn get_task_result(
        &self,
        mut request: GetTaskPayloadParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        let mapping = self.task_mapping(&request.task_id)?;
        let server = mapping.upstream_server.clone();
        let subject = self.authorize_mapped_task(&context, GatewayAction::TasksResult, &mapping)?;
        request.task_id = mapping.upstream_task_id.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::GetTaskPayloadRequest(
                GetTaskPayloadRequest::new(request),
            ))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::GetTaskPayloadResult(mut result) => {
                project_task_payload_result(&mut result, &mapping)?;
                Ok(result)
            }
            ServerResult::CallToolResult(result) => {
                let payload = serde_json::to_value(result)
                    .map_err(|err| mcp_internal(format!("failed to encode task payload: {err}")))?;
                let mut result = GetTaskPayloadResult::new(payload);
                project_task_payload_result(&mut result, &mapping)?;
                Ok(result)
            }
            ServerResult::CustomResult(result) => {
                let mut result = GetTaskPayloadResult::new(result.0);
                project_task_payload_result(&mut result, &mapping)?;
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tasks/result", other)),
        }
    }

    async fn cancel_task(
        &self,
        mut request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        let mapping = self.task_mapping(&request.task_id)?;
        let server = mapping.upstream_server.clone();
        let subject = self.authorize_mapped_task(&context, GatewayAction::TasksCancel, &mapping)?;
        request.task_id = mapping.upstream_task_id.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::CancelTaskRequest(CancelTaskRequest::new(
                request,
            )))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::CancelTaskResult(mut result) => {
                result.task.task_id = mapping.gateway_task_id.to_string();
                Ok(result)
            }
            ServerResult::GetTaskResult(upstream_result) => {
                let mut task = upstream_result.task;
                task.task_id = mapping.gateway_task_id.to_string();
                let mut result = CancelTaskResult::new(task);
                result.meta = upstream_result.meta;
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tasks/cancel", other)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GatewayUpstreamHandler {
    profile_id: GatewayProfileId,
    principal_id: PrincipalId,
    upstream_server: ServerSlug,
    state: GatewayState,
    downstream: Peer<RoleServer>,
}

impl ClientHandler for GatewayUpstreamHandler {
    fn get_info(&self) -> ClientInfo {
        let mut info = ClientInfo::default();
        info.client_info =
            Implementation::new("veoveo-gateway-upstream", env!("CARGO_PKG_VERSION"));
        info
    }

    async fn on_progress(
        &self,
        params: rmcp::model::ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        if let Err(err) = self.downstream.notify_progress(params).await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward progress notification: {err}"
            );
        }
    }

    async fn on_resource_updated(
        &self,
        mut params: rmcp::model::ResourceUpdatedNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        match project_upstream_resource_for_owner(
            &self.state,
            &self.profile_id,
            &self.principal_id,
            &self.upstream_server,
            &params.uri,
        ) {
            Ok(Some(projection)) => {
                params.uri = projection.gateway_uri.to_string();
            }
            Ok(None) => {
                tracing::warn!(
                    upstream_server = %self.upstream_server,
                    upstream_uri = %params.uri,
                    "dropped resource update notification for unmapped upstream resource"
                );
                return;
            }
            Err(err) => {
                tracing::warn!(
                    upstream_server = %self.upstream_server,
                    upstream_uri = %params.uri,
                    "failed to project resource update notification: {err}"
                );
                return;
            }
        }
        if let Err(err) = self.downstream.notify_resource_updated(params).await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward resource update notification: {err}"
            );
        }
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<RoleClient>) {
        if let Err(err) = self.downstream.notify_resource_list_changed().await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward resource list notification: {err}"
            );
        }
    }

    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        if let Err(err) = self.downstream.notify_tool_list_changed().await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward tool list notification: {err}"
            );
        }
    }

    async fn on_prompt_list_changed(&self, _context: NotificationContext<RoleClient>) {
        if let Err(err) = self.downstream.notify_prompt_list_changed().await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward prompt list notification: {err}"
            );
        }
    }

    async fn on_task_status(
        &self,
        mut params: TaskStatusNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        let Ok(upstream_task_id) = UpstreamTaskId::new(params.task.task_id.clone()) else {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "dropped task status notification with invalid upstream task id"
            );
            return;
        };
        let mapping = match self
            .state
            .task_mapping_by_upstream(&self.upstream_server, &upstream_task_id)
        {
            Ok(Some(mapping)) => mapping,
            Ok(None) => {
                tracing::warn!(
                    upstream_server = %self.upstream_server,
                    upstream_task_id = %upstream_task_id,
                    "dropped task status notification for unknown gateway task mapping"
                );
                return;
            }
            Err(err) => {
                tracing::warn!(
                    upstream_server = %self.upstream_server,
                    "failed to read gateway task mapping for notification: {err}"
                );
                return;
            }
        };
        if !task_mapping_allows_principal(&self.profile_id, &mapping, &self.principal_id) {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                upstream_task_id = %upstream_task_id,
                "dropped task status notification for another gateway principal"
            );
            return;
        }
        params.task.task_id = mapping.gateway_task_id.to_string();
        let notification = ServerNotification::TaskStatusNotification(Notification::new(params));
        if let Err(err) = self.downstream.send_notification(notification).await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward task status notification: {err}"
            );
        }
    }
}

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

fn resource_read_action(uri: &str) -> GatewayAction {
    match resource_read_kind(uri) {
        ResourceReadKind::Artifact => GatewayAction::ArtifactRead,
        ResourceReadKind::Usage => GatewayAction::UsageRead,
        ResourceReadKind::General => GatewayAction::ResourcesRead,
    }
}

fn parse_gateway_tool(
    catalog: &GatewayCatalog,
    name: &str,
) -> Result<crate::GatewayToolProjection, McpError> {
    let gateway_name = GatewayToolName::new(name.to_string())
        .map_err(|err| mcp_invalid_params(format!("invalid gateway tool name: {err}")))?;
    catalog
        .parse_tool_name(&gateway_name)
        .map_err(|err| mcp_invalid_params(err.to_string()))
}

fn ensure_unique_prompts(prompts: &[rmcp::model::Prompt]) -> Result<(), McpError> {
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

fn upstream_error(err: impl fmt::Display) -> McpError {
    mcp_internal(format!("upstream MCP request failed: {err}"))
}

fn unexpected_upstream_response(method: &str, response: ServerResult) -> McpError {
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

fn mcp_invalid_request(message: impl Into<String>) -> McpError {
    McpError::invalid_request(message.into(), None)
}

fn mcp_invalid_params(message: impl Into<String>) -> McpError {
    McpError::invalid_params(message.into(), None)
}

fn mcp_internal(message: impl Into<String>) -> McpError {
    McpError::internal_error(message.into(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let output: GenerationRunOutput =
            serde_json::from_value(result.structured_content.unwrap()).unwrap();
        assert_eq!(
            output.artifacts[0].metadata["task_id"].as_str(),
            Some("gateway-task-1")
        );
    }
}
