use chrono::Utc;
use rmcp::{
    model::ErrorData as McpError,
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{
    AuditEvent, CanonicalTaskId, CompatibilityHelperId, GatewayAction, GatewayResourceProjection,
    LocalToolName, OAuthClientRegistration, OAuthClientSurface, PolicyDecision, PolicyEffect,
    PolicyReasonCode, PolicyTarget, PrincipalAuditAttributes, PromptName, ServerSlug, TraceId,
    trace_id_from_traceparent,
};
use veoveo_platform_store::{
    RecordIdKey, deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
};

use crate::{
    AuthenticatedSubject, PolicyRequest,
    mcp_support::{
        audit_method_name, gateway_resource_uri, mcp_internal, mcp_invalid_params,
        mcp_invalid_request, project_upstream_resource, resource_policy_target,
    },
    principal_audit_metadata,
};

use super::GatewayMcp;

pub(super) struct CanonicalTaskRoute {
    pub(super) task_id: String,
    pub(super) server: ServerSlug,
    pub(super) subject: AuthenticatedSubject,
}

impl GatewayMcp {
    pub(super) async fn authorize_canonical_task(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        task_id: &str,
    ) -> Result<CanonicalTaskRoute, McpError> {
        let subject = self.authenticated(context)?;
        let trace_id = trace_id_for_context(context)?;
        self.authorize_canonical_task_with_trace(&subject, action, task_id, Some(trace_id))
            .await
    }

    pub(super) async fn authorize_canonical_task_for_subject(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        task_id: &str,
    ) -> Result<CanonicalTaskRoute, McpError> {
        self.authorize_canonical_task_with_trace(subject, action, task_id, None)
            .await
    }

    async fn authorize_canonical_task_with_trace(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        task_id: &str,
        trace_id: Option<TraceId>,
    ) -> Result<CanonicalTaskRoute, McpError> {
        let canonical_task_id = CanonicalTaskId::new(task_id.to_owned())
            .map_err(|error| mcp_invalid_params(format!("invalid canonical task id: {error}")))?;
        let Some(route) = self
            .state
            .task_route(&canonical_task_id)
            .await
            .map_err(|error| {
                mcp_internal(format!("failed to read canonical task route: {error}"))
            })?
        else {
            tracing::warn!(%task_id, "canonical task record was not found");
            return Err(mcp_invalid_params("unknown task id"));
        };
        let tenant_key = subject.authority.tenant.as_str();
        let expected_tenant = deterministic_tenant_id(tenant_key)
            .map_err(|error| mcp_internal(format!("invalid task tenant: {error}")))?
            .record_id();
        let expected_owner = deterministic_principal_id(tenant_key, subject.actor.id.as_str())
            .map_err(|error| mcp_internal(format!("invalid task owner: {error}")))?
            .record_id();
        let expected_work_context =
            deterministic_work_context_id(tenant_key, subject.authority.work_context.as_str())
                .map_err(|error| mcp_internal(format!("invalid task Work Context: {error}")))?
                .record_id();
        let authority_digest = hex::encode(super::invocation_authorization_fingerprint(
            &subject.actor,
            &subject.authority,
        )?);
        if route.tenant != expected_tenant
            || route.owner != expected_owner
            || route.work_context != expected_work_context
            || record_key(&route.profile)? != self.profile_id.as_str()
            || route.authority_digest != authority_digest
        {
            tracing::warn!(
                %task_id,
                caller_actor = %subject.actor.id,
                caller_initiator = %subject.principal.id,
                caller_profile = %self.profile_id,
                caller_tenant = ?subject.actor.tenant,
                "canonical task ownership did not match the authenticated subject"
            );
            return Err(mcp_invalid_params("unknown task id"));
        }
        let server = ServerSlug::new(record_key(&route.server)?)
            .map_err(|error| mcp_internal(format!("task has invalid server: {error}")))?;
        let exposed = self
            .catalog
            .current()
            .profile_servers(&self.profile_id)
            .into_iter()
            .any(|(exposure, manifest)| {
                manifest.slug == server
                    && exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled
                    && manifest.capabilities.tasks
            });
        if !exposed {
            tracing::warn!(
                %task_id,
                %server,
                profile = %self.profile_id,
                "canonical task server is not exposed with tasks enabled"
            );
            return Err(mcp_invalid_params("unknown task id"));
        }
        let target = PolicyTarget::Task {
            server: server.clone(),
            task_id: canonical_task_id,
        };
        let subject = match trace_id {
            Some(trace_id) => {
                self.authorize_subject_with_trace(subject, action, target, trace_id)
                    .await?
            }
            None => self.authorize_subject(subject, action, target).await?,
        };
        Ok(CanonicalTaskRoute {
            task_id: route.source_task_id,
            server,
            subject,
        })
    }

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

    pub(super) fn authenticated_oauth_client(
        &self,
        subject: &AuthenticatedSubject,
    ) -> Result<OAuthClientRegistration, McpError> {
        self.catalog
            .current()
            .oauth_client(&subject.access_token.oauth_client_id)
            .cloned()
            .ok_or_else(|| mcp_invalid_request("authenticated OAuth client is not registered"))
    }

    pub(super) fn is_compatibility_helper(
        &self,
        server: &ServerSlug,
        tool: &LocalToolName,
    ) -> bool {
        self.catalog.current().is_compatibility_helper(server, tool)
    }

    pub(super) fn client_allows_compatibility_helper(
        &self,
        subject: &AuthenticatedSubject,
        server: &ServerSlug,
        tool: &LocalToolName,
    ) -> Result<bool, McpError> {
        if !self.is_compatibility_helper(server, tool) {
            return Ok(true);
        }
        let client = self.authenticated_oauth_client(subject)?;
        if client.client_surface != OAuthClientSurface::ToolsCompat {
            return Ok(false);
        }
        let helper = CompatibilityHelperId::new(format!("{server}.{tool}")).map_err(|err| {
            mcp_internal(format!("failed to build compatibility helper id: {err}"))
        })?;
        Ok(client.allowed_compatibility_helpers.contains(&helper))
    }

    pub(super) fn client_allows_task_projection(
        &self,
        subject: &AuthenticatedSubject,
    ) -> Result<bool, McpError> {
        let client = self.authenticated_oauth_client(subject)?;
        Ok(client_surface_allows_task_projection(
            client.client_surface,
            client.direct_task_call_adapter,
        ))
    }

    pub(super) fn client_uses_direct_task_call_adapter(
        &self,
        subject: &AuthenticatedSubject,
    ) -> Result<bool, McpError> {
        let client = self.authenticated_oauth_client(subject)?;
        Ok(client.client_surface == OAuthClientSurface::ToolsCompat
            && client.direct_task_call_adapter)
    }

    pub(super) async fn authorize(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<AuthenticatedSubject, McpError> {
        let subject = self.authenticated(context)?;
        let trace_id = trace_id_for_context(context)?;
        self.authorize_subject_with_trace(&subject, action, target, trace_id)
            .await
    }

    pub(super) async fn authorize_subject(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<AuthenticatedSubject, McpError> {
        let (subject, decision) = self
            .evaluate_policy_for_subject(subject, action, target)
            .await?;
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

    async fn authorize_subject_with_trace(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: PolicyTarget,
        trace_id: TraceId,
    ) -> Result<AuthenticatedSubject, McpError> {
        let (subject, decision) = self
            .evaluate_policy_for_subject_with_trace(subject, action, target, trace_id)
            .await?;
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

    pub(super) async fn allows(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<bool, McpError> {
        let subject = self.authenticated(context)?;
        let trace_id = trace_id_for_context(context)?;
        let (_subject, decision) = self
            .evaluate_policy_for_subject_with_trace(&subject, action, target, trace_id)
            .await?;
        Ok(decision.effect == PolicyEffect::Allow)
    }

    pub(super) async fn evaluate_policy_for_subject(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<(AuthenticatedSubject, PolicyDecision), McpError> {
        let trace_id = TraceId::new(uuid::Uuid::new_v4().to_string())
            .map_err(|err| mcp_internal(format!("failed to create trace id: {err}")))?;
        self.evaluate_policy_for_subject_with_trace(subject, action, target, trace_id)
            .await
    }

    async fn evaluate_policy_for_subject_with_trace(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: PolicyTarget,
        trace_id: TraceId,
    ) -> Result<(AuthenticatedSubject, PolicyDecision), McpError> {
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
            .await
            .map_err(|err| mcp_internal(format!("failed to record gateway audit event: {err}")))?;
        Ok((subject.clone(), decision))
    }

    pub(super) async fn authorize_tool(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        tool: LocalToolName,
    ) -> Result<(AuthenticatedSubject, TraceId), McpError> {
        let subject = self.authenticated(context)?;
        let trace_id = trace_id_for_context(context)?;
        let subject = self
            .authorize_subject_with_trace(
                &subject,
                action,
                PolicyTarget::Tool { server, tool },
                trace_id.clone(),
            )
            .await?;
        Ok((subject, trace_id))
    }

    pub(super) async fn allows_tool(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        tool: LocalToolName,
    ) -> Result<bool, McpError> {
        self.allows(context, action, PolicyTarget::Tool { server, tool })
            .await
    }

    pub(super) async fn authorize_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        uri: &str,
    ) -> Result<AuthenticatedSubject, McpError> {
        let target = resource_policy_target(server, uri)?;
        self.authorize(context, action, target).await
    }

    pub(super) async fn authorize_projected_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        projection: &GatewayResourceProjection,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize_resource(
            context,
            action,
            projection.server.clone(),
            projection.gateway_uri.as_str(),
        )
        .await
    }

    pub(super) async fn allows_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        uri: &str,
    ) -> Result<bool, McpError> {
        let target = resource_policy_target(server, uri)?;
        self.allows(context, action, target).await
    }

    pub(super) async fn authorize_prompt(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        prompt: PromptName,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize(context, action, PolicyTarget::Prompt { server, prompt })
            .await
    }

    pub(super) async fn allows_prompt(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        prompt: PromptName,
    ) -> Result<bool, McpError> {
        self.allows(context, action, PolicyTarget::Prompt { server, prompt })
            .await
    }

    pub(super) async fn record_policy_denial(
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
            .await
            .map_err(|err| mcp_internal(format!("failed to record gateway audit event: {err}")))?;
        Ok(())
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
        Ok(GatewayResourceProjection {
            server,
            gateway_uri: gateway_resource_uri(uri)?,
            upstream_uri: gateway_resource_uri(uri)?,
        })
    }

    pub(super) fn project_upstream_resource(
        &self,
        server: &ServerSlug,
        uri: &str,
    ) -> Result<GatewayResourceProjection, McpError> {
        let catalog = self.catalog.current();
        let manifest = catalog
            .server(server)
            .ok_or_else(|| mcp_internal(format!("unknown upstream server `{server}`")))?;
        project_upstream_resource(manifest, uri)
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

fn trace_id_for_context(context: &RequestContext<RoleServer>) -> Result<TraceId, McpError> {
    let value = context
        .meta
        .get_traceparent()
        .and_then(trace_id_from_traceparent)
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    TraceId::new(value).map_err(|error| mcp_internal(format!("failed to create trace id: {error}")))
}

fn record_key(record: &veoveo_platform_store::RecordId) -> Result<String, McpError> {
    match &record.key {
        RecordIdKey::String(value) => Ok(value.clone()),
        RecordIdKey::Uuid(value) => Ok(value.to_string()),
        RecordIdKey::Number(value) => Ok(value.to_string()),
        other => Err(mcp_internal(format!(
            "gateway task route has unsupported record key {other:?}"
        ))),
    }
}

fn client_surface_allows_task_projection(
    surface: OAuthClientSurface,
    direct_task_call_adapter: bool,
) -> bool {
    match surface {
        OAuthClientSurface::FullMcp => true,
        OAuthClientSurface::ToolsCompat => direct_task_call_adapter,
    }
}

#[cfg(test)]
mod tests {
    use veoveo_mcp_contract::OAuthClientSurface;

    use super::client_surface_allows_task_projection;

    #[test]
    fn full_mcp_always_receives_canonical_tasks() {
        assert!(client_surface_allows_task_projection(
            OAuthClientSurface::FullMcp,
            false
        ));
        assert!(client_surface_allows_task_projection(
            OAuthClientSurface::FullMcp,
            true
        ));
    }

    #[test]
    fn tools_compat_requires_explicit_task_projection() {
        assert!(!client_surface_allows_task_projection(
            OAuthClientSurface::ToolsCompat,
            false
        ));
        assert!(client_surface_allows_task_projection(
            OAuthClientSurface::ToolsCompat,
            true
        ));
    }
}
