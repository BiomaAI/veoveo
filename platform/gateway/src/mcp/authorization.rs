use rmcp::{
    model::ErrorData as McpError,
    service::{RequestContext, RoleServer},
};
use veoveo_audit_contract::{
    AuditDetail, AuditOutcome, AuditReadMethod, AuditReason, TaskActivity,
};
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::{
    CanonicalTaskId, CompatibilityHelperId, GatewayResourceProjection, LocalToolName,
    OAuthClientRegistration, OAuthClientSurface, PolicyDecision, PolicyEffect, PolicyReasonCode,
    PolicyTarget, PromptName, ServerSlug, TraceId,
};
use veoveo_platform_store::{
    RecordIdKey, deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
};

use crate::{
    AuthenticatedSubject, PolicyRequest,
    mcp_support::{
        gateway_resource_uri, mcp_internal, mcp_invalid_params, mcp_invalid_request,
        project_upstream_resource, resource_policy_target,
    },
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
        let canonical_task_id = CanonicalTaskId::parse(task_id)
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
        if route.tenant != expected_tenant
            || route.owner != expected_owner
            || route.work_context != expected_work_context
            || record_key(&route.profile)? != self.profile_id.as_str()
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
        let ownership = self
            .state
            .task_route_ownership(&route)
            .await
            .map_err(|error| {
                mcp_internal(format!("failed to read retained Task ownership: {error}"))
            })?;
        let owned = match ownership {
            Some(ownership) => ownership.allows(&subject.actor, &subject.authority),
            // A version-0 external route has no recoverable ownership metadata.
            // Preserve its exact admission constraint until its original expiry.
            None => {
                route.authority_digest
                    == hex::encode(super::invocation_authorization_fingerprint(
                        &subject.actor,
                        &subject.authority,
                    )?)
            }
        };
        if !owned {
            return Err(mcp_invalid_params("unknown task id"));
        }
        let server = ServerSlug::parse(record_key(&route.server)?)
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
        let mut subject = parts
            .extensions
            .get::<AuthenticatedSubject>()
            .cloned()
            .ok_or_else(|| mcp_invalid_request("authenticated subject missing"))?;
        let observation = parts
            .extensions
            .get::<crate::request_observation::RequestObservation>()
            .ok_or_else(|| mcp_invalid_request("HTTP request correlation missing"))?;
        subject.audit = observation.audit.clone();
        Ok(subject)
    }

    pub(super) async fn authenticated_oauth_client(
        &self,
        subject: &AuthenticatedSubject,
    ) -> Result<OAuthClientRegistration, McpError> {
        self.state
            .effective_oauth_client(
                &self.catalog.current(),
                &subject.access_token.oauth_client_id,
            )
            .await
            .map_err(|_| mcp_invalid_request("current OAuth registration is unavailable"))?
            .map(|client| client.registration)
            .ok_or_else(|| mcp_invalid_request("authenticated OAuth client is not registered"))
    }

    pub(super) fn is_compatibility_helper(
        &self,
        server: &ServerSlug,
        tool: &LocalToolName,
    ) -> bool {
        self.catalog.current().is_compatibility_helper(server, tool)
    }

    pub(super) async fn client_allows_compatibility_helper(
        &self,
        subject: &AuthenticatedSubject,
        server: &ServerSlug,
        tool: &LocalToolName,
    ) -> Result<bool, McpError> {
        if !self.is_compatibility_helper(server, tool) {
            return Ok(true);
        }
        let client = self.authenticated_oauth_client(subject).await?;
        if client.client_surface != OAuthClientSurface::ToolsCompat {
            return Ok(false);
        }
        let helper = CompatibilityHelperId::parse(format!("{server}.{tool}")).map_err(|err| {
            mcp_internal(format!("failed to build compatibility helper id: {err}"))
        })?;
        Ok(client.allowed_compatibility_helpers.contains(&helper))
    }

    pub(super) async fn client_allows_task_projection(
        &self,
        subject: &AuthenticatedSubject,
    ) -> Result<bool, McpError> {
        let client = self.authenticated_oauth_client(subject).await?;
        Ok(client_surface_allows_task_projection(
            client.client_surface,
            client.direct_task_call_adapter,
        ))
    }

    pub(super) async fn client_uses_direct_task_call_adapter(
        &self,
        subject: &AuthenticatedSubject,
    ) -> Result<bool, McpError> {
        let client = self.authenticated_oauth_client(subject).await?;
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
            Err(mcp_invalid_request(policy_denial_message(&decision)))
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
            Err(mcp_invalid_request(policy_denial_message(&decision)))
        }
    }

    pub(super) async fn evaluate_policy_for_subject(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<(AuthenticatedSubject, PolicyDecision), McpError> {
        let trace_id = TraceId::parse(&subject.audit.trace_id)
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
        let decision = self
            .policy_decision(subject, action, &target, trace_id)
            .await?;
        // Owned task status is a polling protocol. Its denials are still records.
        if action != GatewayAction::TasksGet || decision.effect != PolicyEffect::Allow {
            let detail = match action {
                GatewayAction::ToolsCall => AuditDetail::ToolAdmission,
                GatewayAction::ResourcesRead | GatewayAction::ArtifactRead => AuditDetail::Read {
                    method: AuditReadMethod::ResourceRead,
                },
                GatewayAction::PromptsGet => AuditDetail::Read {
                    method: AuditReadMethod::PromptGet,
                },
                GatewayAction::CompletionComplete => AuditDetail::Read {
                    method: AuditReadMethod::Completion,
                },
                GatewayAction::SubscriptionsListen => AuditDetail::Read {
                    method: AuditReadMethod::Subscription,
                },
                GatewayAction::TasksGet => AuditDetail::Read {
                    method: AuditReadMethod::Status,
                },
                GatewayAction::TasksUpdate => AuditDetail::Task {
                    activity: TaskActivity::Update,
                },
                GatewayAction::TasksCancel => AuditDetail::Task {
                    activity: TaskActivity::Cancel,
                },
                GatewayAction::UsageRead => AuditDetail::Read {
                    method: AuditReadMethod::Usage,
                },
                _ => return Err(mcp_internal("action requires its domain audit producer")),
            };
            let allowed = decision.effect == PolicyEffect::Allow;
            let target = crate::audit::mcp_audit_target(&target)
                .map_err(|_| mcp_internal("invalid audit target"))?;
            let draft = subject
                .audit_draft(
                    &self.profile_id,
                    target,
                    detail,
                    if allowed {
                        AuditOutcome::Allowed
                    } else {
                        AuditOutcome::Denied
                    },
                    if allowed {
                        AuditReason::Accepted
                    } else {
                        crate::audit::policy_reason(decision.reason)
                    },
                )
                .map_err(|_| mcp_internal("invalid audit attribution"))?;
            self.state.record_audit(draft).await.map_err(|_| {
                mcp_internal("The gateway couldn't commit the audit record. Try again shortly.")
            })?;
        }
        Ok((subject.clone(), decision))
    }

    /// Discovery evaluates item visibility here; the list handler owns its one
    /// request record after aggregation, including responses served from cache.
    pub(super) async fn allows_catalog_targets(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        targets: Vec<PolicyTarget>,
    ) -> Result<Vec<bool>, McpError> {
        let subject = self.authenticated(context)?;
        let trace = trace_id_for_context(context)?;
        let mut admitted = Vec::with_capacity(targets.len());
        for target in targets {
            admitted.push(
                self.policy_decision(&subject, action, &target, trace.clone())
                    .await?
                    .effect
                    == PolicyEffect::Allow,
            );
        }
        Ok(admitted)
    }

    async fn policy_decision(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: &PolicyTarget,
        trace_id: TraceId,
    ) -> Result<PolicyDecision, McpError> {
        let catalog = self.catalog.current();
        let managed_admitted = self
            .state
            .oauth_action_admitted(&catalog, subject, action, target)
            .await
            .map_err(|_| mcp_internal("OAuth client authority unavailable"))?;
        let mut decision = catalog.decide(PolicyRequest {
            principal: &subject.principal,
            profile: &self.profile_id,
            action: action.into(),
            target,
            trace_id: &trace_id,
        });
        if !managed_admitted
            || !super::knowledge_indexing::allows_action(&catalog, subject, action, target)
        {
            decision.effect = PolicyEffect::Deny;
            decision.reason = PolicyReasonCode::PolicyDeny;
        }
        Ok(decision)
    }

    pub(super) async fn record_discovery(
        &self,
        subject: &AuthenticatedSubject,
        collection: veoveo_audit_contract::DiscoveryKind,
        mut identities: Vec<String>,
        denied: u32,
    ) -> Result<(), McpError> {
        use sha2::{Digest, Sha256};
        use veoveo_audit_contract::AuditTarget;
        identities.sort();
        let visible = identities
            .len()
            .try_into()
            .map_err(|_| mcp_internal("catalog exceeds audit count range"))?;
        // A length-delimited JSON array prevents concatenation collisions between IDs.
        let bytes = serde_json::to_vec(&identities)
            .map_err(|_| mcp_internal("invalid catalog identity"))?;
        let visible_digest = veoveo_types::Sha256Digest::from_bytes(Sha256::digest(bytes).into());
        let draft = subject
            .audit_draft(
                &self.profile_id,
                AuditTarget::Discovery {
                    server: None,
                    collection,
                },
                AuditDetail::Discovery {
                    collection,
                    visible,
                    denied,
                    visible_digest,
                },
                AuditOutcome::Allowed,
                AuditReason::Accepted,
            )
            .map_err(|_| mcp_internal("invalid discovery attribution"))?;
        self.state
            .record_audit(draft)
            .await
            .map_err(|_| mcp_internal("required discovery audit unavailable"))
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

    /// A successful read is recorded with its returned observation by the read
    /// handler. Denials commit here, before any upstream request.
    pub(super) async fn admit_resource_read(
        &self,
        context: &RequestContext<RoleServer>,
        projection: &GatewayResourceProjection,
    ) -> Result<AuthenticatedSubject, McpError> {
        let subject = self.authenticated(context)?;
        let action = crate::mcp_support::resource_read_action(projection.gateway_uri.as_str());
        let target =
            resource_policy_target(projection.server.clone(), projection.gateway_uri.as_str())?;
        let decision = self
            .policy_decision(&subject, action, &target, trace_id_for_context(context)?)
            .await?;
        if decision.effect == PolicyEffect::Deny {
            self.record_policy_denial(&subject, action, target, decision.reason)
                .await?;
            return Err(mcp_invalid_request(policy_denial_message(&decision)));
        }
        Ok(subject)
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

    pub(super) async fn record_policy_denial(
        &self,
        subject: &AuthenticatedSubject,
        action: GatewayAction,
        target: PolicyTarget,
        reason: PolicyReasonCode,
    ) -> Result<(), McpError> {
        let target = crate::audit::mcp_audit_target(&target)
            .map_err(|_| mcp_internal("invalid audit target"))?;
        let detail = match action {
            GatewayAction::ToolsCall => AuditDetail::ToolAdmission,
            GatewayAction::ResourcesRead | GatewayAction::ArtifactRead => AuditDetail::Read {
                method: AuditReadMethod::ResourceRead,
            },
            _ => AuditDetail::Read {
                method: AuditReadMethod::Status,
            },
        };
        let draft = subject
            .audit_draft(
                &self.profile_id,
                target,
                detail,
                AuditOutcome::Denied,
                crate::audit::policy_reason(reason),
            )
            .map_err(|_| mcp_internal("invalid denial attribution"))?;
        self.state
            .record_audit(draft)
            .await
            .map_err(|_| mcp_internal("required denial audit unavailable"))?;
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
        let prompt = PromptName::parse(prompt)
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
    let parts = context
        .extensions
        .get::<axum::http::request::Parts>()
        .ok_or_else(|| mcp_invalid_request("HTTP request correlation missing"))?;
    let observation = parts
        .extensions
        .get::<crate::request_observation::RequestObservation>()
        .ok_or_else(|| mcp_invalid_request("HTTP request correlation missing"))?;
    TraceId::parse(&observation.audit.trace_id).map_err(|_| mcp_internal("invalid request trace"))
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

/// Caller-facing text for a policy denial. The reason code is mapped to words so
/// callers never see a Rust variant name; the full decision stays in the audit log.
fn policy_denial_message(decision: &PolicyDecision) -> String {
    let action = match &decision.target {
        PolicyTarget::Tool { server, tool } => format!("call `{server}__{tool}`"),
        PolicyTarget::Resource { uri, .. } => format!("read `{uri}`"),
        PolicyTarget::ResourceTemplate { uri, .. } => format!("use resource template `{uri}`"),
        PolicyTarget::Prompt { server, prompt } => format!("use prompt `{prompt}` on `{server}`"),
        PolicyTarget::Task { task_id, .. } => format!("access task `{task_id}`"),
        PolicyTarget::PlatformTask { task_id, .. } => format!("access task `{task_id}`"),
        PolicyTarget::Artifact { artifact_uri, .. } => format!("access `{artifact_uri}`"),
        PolicyTarget::Usage { usage_uri, .. } => format!("read `{usage_uri}`"),
        PolicyTarget::Server { server } => format!("use the `{server}` server"),
        PolicyTarget::Gateway | PolicyTarget::Owner(_) | PolicyTarget::Unadmitted(_) => {
            "make this request".to_owned()
        }
    };
    let detail = match decision.reason {
        PolicyReasonCode::MissingScope | PolicyReasonCode::UnknownScope => {
            " Your access token is missing a scope this profile requires. Ask an administrator \
             to grant it, then sign in again."
        }
        PolicyReasonCode::MissingDataLabel | PolicyReasonCode::UnknownDataLabel => {
            " Your account isn't cleared for the data labels this requires."
        }
        PolicyReasonCode::MissingRole | PolicyReasonCode::MissingGroup => {
            " Your account doesn't have the role or group membership this requires."
        }
        PolicyReasonCode::MissingPrincipalAssurance => {
            " Sign in again with the stronger authentication this profile requires."
        }
        PolicyReasonCode::MissingPrincipal
        | PolicyReasonCode::UnknownPrincipal
        | PolicyReasonCode::MissingTenant
        | PolicyReasonCode::UnknownTenant
        | PolicyReasonCode::UnknownTokenIssuer => {
            " Your identity isn't recognized by this installation. Sign in again."
        }
        PolicyReasonCode::TokenExpired | PolicyReasonCode::TokenNotYetValid => {
            " Your access token isn't currently valid. Sign in again."
        }
        PolicyReasonCode::TokenAudienceMismatch => {
            " Your access token was issued for a different resource. Sign in to this profile."
        }
        PolicyReasonCode::ReplayDetected => " This request was already used. Send a new request.",
        PolicyReasonCode::UnknownProfile
        | PolicyReasonCode::UnknownServer
        | PolicyReasonCode::UnknownTool
        | PolicyReasonCode::UnknownResource
        | PolicyReasonCode::UnknownPrompt
        | PolicyReasonCode::UnknownTask
        | PolicyReasonCode::UnknownArtifact => " It isn't available in this profile.",
        PolicyReasonCode::PolicyDeny | PolicyReasonCode::PolicyAllow => "",
    };
    format!("You don't have permission to {action}.{detail}")
}

#[cfg(test)]
mod tests {
    use veoveo_gateway_contract::GatewayAction;
    use veoveo_mcp_contract::{
        GatewayProfileId, LocalToolName, OAuthClientSurface, PolicyDecision, PolicyReasonCode,
        PolicyTarget, ServerSlug, TraceId,
    };

    use super::{client_surface_allows_task_projection, policy_denial_message};

    #[test]
    fn policy_denial_names_the_tool_and_explains_the_reason() {
        let decision = PolicyDecision::deny(
            GatewayProfileId::parse("operator").unwrap(),
            GatewayAction::ToolsCall,
            PolicyTarget::Tool {
                server: ServerSlug::parse("map").unwrap(),
                tool: LocalToolName::parse("route").unwrap(),
            },
            PolicyReasonCode::MissingScope,
            TraceId::parse("policy-denial-message").unwrap(),
        );
        let message = policy_denial_message(&decision);
        assert!(message.starts_with("You don't have permission to call `map__route`."));
        assert!(message.contains("missing a scope"));
        assert!(!message.contains("MissingScope"));
    }

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
