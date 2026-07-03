use std::collections::BTreeMap;

use chrono::Utc;
use rmcp::{
    model::ErrorData as McpError,
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{
    AuditEvent, GatewayAction, GatewayResourceProjection, GatewayTaskId, GatewayTaskMapping,
    LocalToolName, PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyTarget,
    PrincipalAssurance, PrincipalAuditAttributes, PrincipalId, PrincipalKind, PromptName,
    ServerResourceUri, ServerSlug, TaskIdProjection, TraceId,
};

use crate::{
    AuthenticatedSubject, PolicyRequest,
    mcp_support::{
        audit_method_name, gateway_resource_uri, mcp_internal, mcp_invalid_params,
        mcp_invalid_request, project_upstream_resource_for_owner, resource_policy_target,
        task_mapping_allows_principal,
    },
};

use super::GatewayMcp;

impl GatewayMcp {
    pub(super) fn authenticated(
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

    pub(super) fn authorize(
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

    pub(super) fn allows(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<bool, McpError> {
        let (_subject, decision) = self.evaluate_policy(context, action, target)?;
        Ok(decision.effect == PolicyEffect::Allow)
    }

    pub(super) fn evaluate_policy(
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
                principal_attributes: Some(PrincipalAuditAttributes::from(&subject.principal)),
                tenant: subject.principal.tenant.clone(),
                token_issuer: Some(subject.access_token.issuer.clone()),
                latency_ms: None,
                metadata: principal_audit_metadata(&subject.principal),
            })
            .map_err(|err| mcp_internal(format!("failed to record gateway audit event: {err}")))?;
        Ok((subject, decision))
    }

    pub(super) fn authorize_tool(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        tool: LocalToolName,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize(context, action, PolicyTarget::Tool { server, tool })
    }

    pub(super) fn allows_tool(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        tool: LocalToolName,
    ) -> Result<bool, McpError> {
        self.allows(context, action, PolicyTarget::Tool { server, tool })
    }

    pub(super) fn authorize_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        uri: &str,
    ) -> Result<AuthenticatedSubject, McpError> {
        let target = resource_policy_target(server, uri)?;
        self.authorize(context, action, target)
    }

    pub(super) fn authorize_projected_resource(
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

    pub(super) fn allows_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        uri: &str,
    ) -> Result<bool, McpError> {
        let target = resource_policy_target(server, uri)?;
        self.allows(context, action, target)
    }

    pub(super) fn authorize_prompt(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        prompt: PromptName,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize(context, action, PolicyTarget::Prompt { server, prompt })
    }

    pub(super) fn allows_prompt(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        prompt: PromptName,
    ) -> Result<bool, McpError> {
        self.allows(context, action, PolicyTarget::Prompt { server, prompt })
    }

    pub(super) fn authorize_mapped_task(
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

    pub(super) fn record_policy_denial(
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
                principal_attributes: Some(PrincipalAuditAttributes::from(&subject.principal)),
                tenant: subject.principal.tenant.clone(),
                token_issuer: Some(subject.access_token.issuer.clone()),
                latency_ms: None,
                metadata: principal_audit_metadata(&subject.principal),
            })
            .map_err(|err| mcp_internal(format!("failed to record gateway audit event: {err}")))?;
        Ok(())
    }

    pub(super) fn task_mapping(&self, task_id: &str) -> Result<GatewayTaskMapping, McpError> {
        let gateway_task_id = GatewayTaskId::new(task_id.to_string())
            .map_err(|err| mcp_invalid_params(format!("invalid gateway task id: {err}")))?;
        self.state
            .task_mapping(&gateway_task_id)
            .map_err(|err| mcp_internal(format!("failed to read gateway task mapping: {err}")))?
            .ok_or_else(|| mcp_invalid_params("unknown gateway task id"))
    }

    pub(super) fn server_for_resource(&self, uri: &str) -> Result<ServerSlug, McpError> {
        self.catalog
            .current()
            .server_for_resource_uri(&self.profile_id, uri)
            .map(|(_, server)| server.slug.clone())
            .ok_or_else(|| mcp_invalid_params(format!("resource URI is not exposed: {uri}")))
    }

    pub(super) fn project_resource_for_upstream(
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

    pub(super) fn project_upstream_resource_for_owner(
        &self,
        server: &ServerSlug,
        owner: &PrincipalId,
        uri: &str,
    ) -> Result<Option<GatewayResourceProjection>, McpError> {
        project_upstream_resource_for_owner(&self.state, &self.profile_id, owner, server, uri)
    }

    pub(super) fn server_for_prompt(&self, prompt: &str) -> Result<ServerSlug, McpError> {
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

fn principal_audit_metadata(
    principal: &veoveo_mcp_contract::Principal,
) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "principal_kind".to_string(),
        principal_kind_value(principal.kind).to_string(),
    );
    insert_joined(&mut metadata, "principal_groups", &principal.groups);
    insert_joined(&mut metadata, "principal_roles", &principal.roles);
    insert_joined(&mut metadata, "principal_scopes", &principal.scopes);
    insert_joined(
        &mut metadata,
        "principal_data_labels",
        &principal.data_labels,
    );
    if !principal.assurances.is_empty() {
        metadata.insert(
            "principal_assurances".to_string(),
            principal
                .assurances
                .iter()
                .map(|assurance| match assurance {
                    PrincipalAssurance::UsPerson => "us_person",
                })
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    metadata
}

fn principal_kind_value(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::User => "user",
        PrincipalKind::Service => "service",
    }
}

fn insert_joined<T: ToString>(
    metadata: &mut BTreeMap<String, String>,
    key: &str,
    values: &std::collections::BTreeSet<T>,
) {
    if !values.is_empty() {
        metadata.insert(
            key.to_string(),
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
    }
}
