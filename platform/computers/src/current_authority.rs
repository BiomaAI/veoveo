//! Current policy for accepted execution. Admission tokens and attachment grants
//! have separate lifetimes; their expiry cannot silently cancel accepted work.
use crate::{ComputerError, ComputersStore, Operation, Result, api::Action};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use veoveo_mcp_contract::{
    GatewayAction, GatewayControlPlane, LocalToolName, PolicyDecision, PolicyEffect, PolicyTarget,
    Principal, ServerSlug, TraceId, WorkContextMembershipLevel,
};
use veoveo_platform_store::{
    EnterpriseRecord, PrincipalKind, PrincipalRecord, RecordId, TenantRecord,
    deterministic_enterprise_id, deterministic_principal_id, deterministic_tenant_id,
};
use veoveo_policy::{PolicyCatalog, PolicyCatalogView, PolicyRequest, decide};

pub(crate) const AUTHORITY_LIFETIME: Duration = Duration::from_secs(30);

/// Retained with the dispatch and its audit event; contains no credentials.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionDecision {
    pub control_revision: String,
    pub control_sha256: String,
    pub checked_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub decision: PolicyDecision,
}
impl ExecutionDecision {
    pub(crate) fn validate(&self, operation: &Operation) -> Result<()> {
        let accepted = &operation.execution_authority;
        if self.control_revision.is_empty()
            || self.control_sha256.len() != 64
            || !self.control_sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || self.valid_until <= self.checked_at
            || self.valid_until - self.checked_at > TimeDelta::seconds(30)
            || self.decision.effect != PolicyEffect::Allow
            || self.decision.profile != accepted.profile
            || self.decision.principal.as_ref() != Some(&accepted.request_context.principal.id)
            || self.decision.tenant.as_ref() != Some(&accepted.invocation.tenant)
            || self.decision.action != GatewayAction::ToolsCall
            || self.decision.target != execution_target(operation.action)
            || self.decision.trace_id.as_str() != operation.operation_id.to_string()
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(())
    }
}

fn execution_target(action: Action) -> PolicyTarget {
    PolicyTarget::Tool {
        server: ServerSlug::new("computers").expect("static server"),
        tool: LocalToolName::new(match action {
            Action::Create => "create",
            Action::Start => "start",
            Action::Stop => "stop",
        })
        .expect("static tool"),
    }
}

pub(crate) struct ExecutionPermit {
    pub evidence: ExecutionDecision,
    pub deadline: Instant,
    pub revision_record: RecordId,
    pub tenant: RecordId,
    pub source: RecordId,
    pub actor: RecordId,
}

impl ComputersStore {
    pub(crate) async fn authorize_execution(
        &self,
        operation: &Operation,
    ) -> Result<ExecutionPermit> {
        tokio::time::timeout(
            Duration::from_secs(5),
            self.read_execution_authority(operation),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    async fn read_execution_authority(&self, operation: &Operation) -> Result<ExecutionPermit> {
        let started = Instant::now();
        let checked_at = Utc::now();
        let accepted = &operation.execution_authority;
        accepted.validate()?;
        let source = &accepted.request_context.principal;
        let token = &accepted.request_context.access_token;
        let revision = self
            .platform
            .active_gateway_control_revision()
            .await
            .map_err(|_| ComputerError::Unavailable)?
            .ok_or(ComputerError::Unavailable)?;
        let control: GatewayControlPlane = serde_json::from_value(
            serde_json::to_value(revision.control_plane).map_err(|_| ComputerError::Unavailable)?,
        )
        .map_err(|_| ComputerError::Unavailable)?;
        if crate::identity::digest(&control)? != revision.sha256 {
            return Err(ComputerError::Unavailable);
        }
        let catalog = PolicyCatalog::new(control).map_err(|_| ComputerError::Unavailable)?;
        let control = catalog.control_plane();
        let profile = catalog
            .profile(&accepted.profile)
            .ok_or(ComputerError::Forbidden)?;
        let client = control
            .oauth_clients
            .iter()
            .find(|c| c.id == token.oauth_client_id)
            .ok_or(ComputerError::Forbidden)?;
        let authorization_server = control
            .authorization_servers
            .iter()
            .find(|a| a.id == profile.authorization_server)
            .ok_or(ComputerError::Forbidden)?;
        let context = control
            .work_contexts
            .iter()
            .find(|c| c.id == accepted.invocation.work_context)
            .ok_or(ComputerError::Forbidden)?;
        if token.issuer != authorization_server.issuer
            || client.authorization_server != profile.authorization_server
            || token.audience != profile.protected_resource
            || !client
                .allowed_resources
                .contains(&profile.protected_resource)
            || client.invocation_mode != token.invocation_mode
            || !token.scopes.is_subset(&client.allowed_scopes)
            || context.tenant != accepted.invocation.tenant
            || client
                .tenant
                .as_ref()
                .is_some_and(|tenant| tenant != &context.tenant)
            || !context
                .membership_for(source, &client.id)
                .is_some_and(|m| m.allows(WorkContextMembershipLevel::Contributor))
        {
            return Err(ComputerError::Forbidden);
        }
        // Both the retained output boundary and the current defaults must remain
        // readable. A newer policy cannot widen the accepted work's clearance.
        for output in [&accepted.invocation.output_policy, &context.output_policy] {
            if !output.data_labels.is_subset(&source.data_labels)
                || output
                    .classification
                    .as_ref()
                    .is_some_and(|label| !source.data_labels.contains(label))
            {
                return Err(ComputerError::Forbidden);
            }
        }
        let target = execution_target(operation.action);
        let decision = decide(
            &catalog,
            PolicyRequest {
                principal: source,
                profile: &accepted.profile,
                action: GatewayAction::ToolsCall,
                target: &target,
                trace_id: &TraceId::new(operation.operation_id.to_string()).expect("UUID trace"),
            },
        );
        if decision.effect != PolicyEffect::Allow {
            return Err(ComputerError::Forbidden);
        }
        let tenant = deterministic_tenant_id(accepted.invocation.tenant.as_str())
            .map_err(|_| ComputerError::Forbidden)?
            .record_id();
        let source_id =
            deterministic_principal_id(accepted.invocation.tenant.as_str(), source.id.as_str())
                .map_err(|_| ComputerError::Forbidden)?
                .record_id();
        let actor_id = deterministic_principal_id(
            accepted.invocation.tenant.as_str(),
            accepted.actor.id.as_str(),
        )
        .map_err(|_| ComputerError::Forbidden)?
        .record_id();
        let mut current = self.platform.client()
            .query("SELECT * FROM ONLY $enterprise; SELECT * FROM ONLY $tenant; SELECT * FROM ONLY $source; SELECT * FROM ONLY $actor;")
            .bind(("enterprise", deterministic_enterprise_id().record_id()))
            .bind(("tenant", tenant.clone())).bind(("source", source_id.clone())).bind(("actor", actor_id.clone()))
            .await.map_err(|_| ComputerError::Unavailable)?.check().map_err(|_| ComputerError::Unavailable)?;
        let enterprise: Option<EnterpriseRecord> =
            current.take(0).map_err(|_| ComputerError::Unavailable)?;
        let tenant_record: Option<TenantRecord> =
            current.take(1).map_err(|_| ComputerError::Unavailable)?;
        let source_record: Option<PrincipalRecord> =
            current.take(2).map_err(|_| ComputerError::Unavailable)?;
        let actor_record: Option<PrincipalRecord> =
            current.take(3).map_err(|_| ComputerError::Unavailable)?;
        if !enterprise
            .is_some_and(|r| r.enabled && r.id == deterministic_enterprise_id().record_id())
            || !tenant_record.is_some_and(|r| {
                r.enabled
                    && r.id == tenant
                    && r.enterprise == deterministic_enterprise_id().record_id()
                    && r.slug == accepted.invocation.tenant.as_str()
            })
            || !enabled_principal(source_record, source, &tenant)
            || !enabled_principal(actor_record, &accepted.actor, &tenant)
        {
            return Err(ComputerError::Forbidden);
        }
        let deadline = started + AUTHORITY_LIFETIME;
        if deadline <= Instant::now() {
            return Err(ComputerError::Unavailable);
        }
        Ok(ExecutionPermit {
            evidence: ExecutionDecision {
                control_revision: revision.revision_id,
                control_sha256: revision.sha256,
                checked_at,
                valid_until: checked_at + TimeDelta::seconds(30),
                decision,
            },
            deadline,
            revision_record: revision.id,
            tenant,
            source: source_id,
            actor: actor_id,
        })
    }
}

fn enabled_principal(
    record: Option<PrincipalRecord>,
    expected: &Principal,
    tenant: &RecordId,
) -> bool {
    let kind = match expected.kind {
        veoveo_mcp_contract::PrincipalKind::User => PrincipalKind::User,
        veoveo_mcp_contract::PrincipalKind::Service => PrincipalKind::Service,
    };
    record.is_some_and(|r| {
        r.enabled
            && r.tenant == *tenant
            && r.kind == kind
            && r.issuer == expected.issuer.as_str()
            && r.subject == expected.subject.as_str()
    })
}
