use chrono::Utc;
use rmcp::{
    model::ErrorData as McpError,
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{
    AuditEvent, CanonicalTaskId, CompatibilityHelperId, GatewayAction, GatewayResourceProjection,
    LocalToolName, OAuthClientRegistration, OAuthClientSurface, PolicyDecision, PolicyEffect,
    PolicyReasonCode, PolicyTarget, PrincipalAuditAttributes, PromptName, ServerSlug, TenantId,
    TraceId,
};
use veoveo_mcp_task_extension::ProtocolTaskId;
use veoveo_platform_store::TaskRecord;
use veoveo_task_runtime::TaskSnapshot;

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
    pub(super) task_id: ProtocolTaskId,
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
        let task_id = task_id
            .parse::<ProtocolTaskId>()
            .map_err(|error| mcp_invalid_params(format!("invalid canonical task id: {error}")))?;
        let mut response = self
            .platform_store
            .client()
            .query("SELECT * FROM ONLY $task;")
            .bind(("task", task_id.task_id().record_id()))
            .await
            .map_err(|error| mcp_internal(format!("failed to read canonical task: {error}")))?
            .check()
            .map_err(|error| mcp_internal(format!("canonical task query failed: {error}")))?;
        let record: Option<TaskRecord> = response
            .take(0)
            .map_err(|error| mcp_internal(format!("failed to decode canonical task: {error}")))?;
        let snapshot = record
            .map(TaskSnapshot::try_from)
            .transpose()
            .map_err(|error| mcp_internal(format!("invalid canonical task record: {error}")))?
            .ok_or_else(|| mcp_invalid_params("unknown task id"))?;
        let subject = self.authenticated(context)?;
        let labels = subject
            .principal
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect();
        if !snapshot.owner.allows(
            subject.principal.id.as_str(),
            self.profile_id.as_str(),
            subject.principal.tenant.as_ref().map(TenantId::as_str),
            &labels,
        ) {
            return Err(mcp_invalid_params("unknown task id"));
        }
        let server = ServerSlug::new(snapshot.server)
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
            return Err(mcp_invalid_params("unknown task id"));
        }
        let canonical_task_id = CanonicalTaskId::new(task_id.to_string())
            .map_err(|error| mcp_internal(format!("invalid canonical task id: {error}")))?;
        let subject = self
            .authorize(
                context,
                action,
                PolicyTarget::Task {
                    server: server.clone(),
                    task_id: canonical_task_id,
                },
            )
            .await?;
        Ok(CanonicalTaskRoute {
            task_id,
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

    pub(super) fn client_allows_direct_task_adapter(
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
        let (subject, decision) = self.evaluate_policy(context, action, target).await?;
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
        let (_subject, decision) = self.evaluate_policy(context, action, target).await?;
        Ok(decision.effect == PolicyEffect::Allow)
    }

    pub(super) async fn evaluate_policy(
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
            .await
            .map_err(|err| mcp_internal(format!("failed to record gateway audit event: {err}")))?;
        Ok((subject, decision))
    }

    pub(super) async fn authorize_tool(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        tool: LocalToolName,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize(context, action, PolicyTarget::Tool { server, tool })
            .await
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
