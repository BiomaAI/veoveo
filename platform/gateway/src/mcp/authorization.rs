use chrono::Utc;
use rmcp::{
    model::ErrorData as McpError,
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{
    AuditEvent, CanonicalTaskId, CompatibilityHelperId, GatewayAction, GatewayProfileId,
    GatewayResourceProjection, LocalToolName, OAuthClientRegistration, OAuthClientSurface,
    PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyTarget, Principal,
    PrincipalAuditAttributes, PromptName, ServerSlug, TenantId, TraceId,
};
use veoveo_mcp_task_extension::ProtocolTaskId;
use veoveo_platform_store::TaskRecord;
use veoveo_task_runtime::TaskOwner;
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
        let subject = self.authenticated(context)?;
        self.authorize_canonical_task_for_subject(&subject, action, task_id)
            .await
    }

    pub(super) async fn authorize_canonical_task_for_subject(
        &self,
        subject: &AuthenticatedSubject,
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
            .map_err(|error| mcp_internal(format!("invalid canonical task record: {error}")))?;
        let Some(snapshot) = snapshot else {
            tracing::warn!(%task_id, "canonical task record was not found");
            return Err(mcp_invalid_params("unknown task id"));
        };
        if !task_owner_allows_actor(&snapshot.owner, &subject.actor, &self.profile_id) {
            tracing::warn!(
                %task_id,
                task_owner = %snapshot.owner.principal_key,
                caller_actor = %subject.actor.id,
                caller_initiator = %subject.principal.id,
                task_profile = %snapshot.owner.profile,
                caller_profile = %self.profile_id,
                task_tenant = ?snapshot.owner.tenant_key,
                caller_tenant = ?subject.actor.tenant,
                "canonical task ownership did not match the authenticated subject"
            );
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
            tracing::warn!(
                %task_id,
                %server,
                profile = %self.profile_id,
                "canonical task server is not exposed with tasks enabled"
            );
            return Err(mcp_invalid_params("unknown task id"));
        }
        let canonical_task_id = CanonicalTaskId::new(task_id.to_string())
            .map_err(|error| mcp_internal(format!("invalid canonical task id: {error}")))?;
        let subject = self
            .authorize_subject(
                subject,
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
        self.authorize_subject(&subject, action, target).await
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

    pub(super) async fn allows(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<bool, McpError> {
        let subject = self.authenticated(context)?;
        self.allows_subject(&subject, action, target).await
    }

    pub(super) async fn allows_subject(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<bool, McpError> {
        let (_subject, decision) = self
            .evaluate_policy_for_subject(subject, action, target)
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
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize(context, action, PolicyTarget::Tool { server, tool })
            .await
    }

    pub(super) async fn authorize_tool_for_subject(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        server: ServerSlug,
        tool: LocalToolName,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize_subject(subject, action, PolicyTarget::Tool { server, tool })
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

fn task_owner_allows_actor(
    owner: &TaskOwner,
    actor: &Principal,
    profile: &GatewayProfileId,
) -> bool {
    let labels = actor.data_labels.iter().map(ToString::to_string).collect();
    owner.allows(
        actor.id.as_str(),
        profile.as_str(),
        actor.tenant.as_ref().map(TenantId::as_str),
        &labels,
    )
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
    use std::collections::BTreeSet;

    use chrono::Utc;
    use veoveo_mcp_contract::{
        AccessSubject, DataLabelId, GatewayProfileId, InvocationAuthority, InvocationProvenance,
        OAuthClientSurface, PolicyVersion, Principal, PrincipalId, PrincipalKind, TenantId,
        TokenIssuer, TokenSubject, WorkContextId, WorkContextMembershipLevel,
        WorkContextOutputPolicy,
    };
    use veoveo_task_runtime::TaskOwner;

    use super::{client_surface_allows_task_projection, task_owner_allows_actor};

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
    fn delegated_task_ownership_uses_the_effective_actor() {
        let initiator = principal("https://idp.example.com", "user-1", PrincipalKind::User);
        let actor = principal(
            "https://veoveo.example/oauth",
            "operator-delegated",
            PrincipalKind::Service,
        );
        let profile = GatewayProfileId::new("operator").unwrap();
        let owner = TaskOwner {
            principal_key: actor.id.to_string(),
            principal_kind: veoveo_task_runtime::PrincipalKind::Service,
            issuer: actor.issuer.to_string(),
            subject: actor.subject.to_string(),
            profile: profile.to_string(),
            tenant_key: actor.tenant.as_ref().map(ToString::to_string),
            data_labels: actor.data_labels.iter().map(ToString::to_string).collect(),
            authority: InvocationAuthority {
                work_context: WorkContextId::new("operations").unwrap(),
                tenant: TenantId::new("tenant-a").unwrap(),
                membership: WorkContextMembershipLevel::Contributor,
                policy_revision: PolicyVersion::new("r1").unwrap(),
                output_policy: WorkContextOutputPolicy {
                    owner: AccessSubject::Principal(initiator.id.clone()),
                    initial_grants: Vec::new(),
                    classification: None,
                    data_labels: BTreeSet::new(),
                },
                provenance: InvocationProvenance::Delegated {
                    initiator: initiator.id.clone(),
                    delegation_id: veoveo_mcp_contract::DelegationId::new("delegation-1").unwrap(),
                },
            },
        };

        assert!(task_owner_allows_actor(&owner, &actor, &profile));
        assert!(!task_owner_allows_actor(&owner, &initiator, &profile));
    }

    fn principal(issuer: &str, subject: &str, kind: PrincipalKind) -> Principal {
        let issuer = TokenIssuer::new(issuer).unwrap();
        let subject = TokenSubject::new(subject).unwrap();
        Principal {
            id: PrincipalId::new(format!("{issuer}#{subject}")).unwrap(),
            kind,
            issuer,
            subject,
            tenant: Some(TenantId::new("tenant-a").unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: BTreeSet::from([DataLabelId::new("cui").unwrap()]),
            assurances: BTreeSet::new(),
            authenticated_at: Some(Utc::now()),
        }
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
