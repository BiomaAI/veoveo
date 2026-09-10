//! One current policy and directory snapshot shared by request projections and dispatch.
use crate::current_authority::AUTHORITY_LIFETIME;
use crate::{AcceptedAuthority, ComputerError, ComputersStore, Result};
use chrono::{DateTime, Utc};
use std::time::Instant;
use veoveo_mcp_contract::{
    GatewayAction, GatewayControlPlane, PolicyDecision, PolicyTarget, Principal, TraceId,
    WorkContextMembershipLevel,
};
use veoveo_platform_store::{
    EnterpriseRecord, PrincipalKind, PrincipalRecord, RecordId, TenantRecord,
    deterministic_enterprise_id, deterministic_principal_id, deterministic_tenant_id,
};
use veoveo_policy::{PolicyCatalog, PolicyCatalogView, PolicyRequest, decide};

pub(crate) struct AuthoritySnapshot {
    pub accepted: AcceptedAuthority,
    pub catalog: PolicyCatalog,
    pub membership: WorkContextMembershipLevel,
    pub checked_at: DateTime<Utc>,
    pub deadline: Instant,
    pub control_revision: String,
    pub control_sha256: String,
    pub revision_record: RecordId,
    pub tenant: RecordId,
    pub source: RecordId,
    pub actor: RecordId,
}
impl AuthoritySnapshot {
    pub fn decision(
        &self,
        action: GatewayAction,
        target: &PolicyTarget,
        trace: &TraceId,
    ) -> PolicyDecision {
        decide(
            &self.catalog,
            PolicyRequest {
                principal: &self.accepted.request_context.principal,
                profile: &self.accepted.profile,
                action,
                target,
                trace_id: trace,
            },
        )
    }
    pub fn check_fresh(&self) -> Result<()> {
        if self.deadline <= Instant::now() {
            Err(ComputerError::Unavailable)
        } else {
            Ok(())
        }
    }
}
impl ComputersStore {
    pub(crate) async fn read_authority(
        &self,
        accepted: &AcceptedAuthority,
    ) -> Result<AuthoritySnapshot> {
        let started = Instant::now();
        let checked_at = Utc::now();
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
        let membership = context
            .membership_for(source, &client.id)
            .ok_or(ComputerError::Forbidden)?
            .min(accepted.invocation.membership);
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
        Ok(AuthoritySnapshot {
            accepted: accepted.clone(),
            catalog,
            membership,
            checked_at,
            deadline: started + AUTHORITY_LIFETIME,
            control_revision: revision.revision_id,
            control_sha256: revision.sha256,
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
