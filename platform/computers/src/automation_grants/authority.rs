use crate::{
    Computer, ComputerActor, ComputerError, ComputersStore, Result,
    api::{AutomationExecutionLimits, AutomationPermission},
    authority_snapshot::AuthoritySnapshot,
    identity::owner_key,
    model::{ComputerRecord, computer_record},
};
use chrono::{DateTime, TimeDelta, Utc};
use std::time::{Duration, Instant};
use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayAction, LocalToolName, PolicyEffect, PolicyTarget, ResourceUri, ServerSlug, TraceId,
    WorkContextMembershipLevel,
};
use veoveo_platform_store::{PrincipalKind, gateway_refresh_family_record_id};

/// A short current read of named authority. This is not a native dispatch ticket.
/// Operation admission and dispatch must also compare its durable grant revision.
pub struct AutomationAuthority {
    computer: Computer,
    grant_id: Uuid,
    grant_revision: u64,
    permission: AutomationPermission,
    limits: Option<AutomationExecutionLimits>,
    deadline: Instant,
    expires_at: DateTime<Utc>,
}
impl AutomationAuthority {
    pub fn computer(&self) -> Result<&Computer> {
        self.check_fresh()?;
        Ok(&self.computer)
    }
    pub fn grant_id(&self) -> Uuid {
        self.grant_id
    }
    pub fn grant_revision(&self) -> u64 {
        self.grant_revision
    }
    pub fn execution_limits(&self) -> Result<Option<AutomationExecutionLimits>> {
        self.check_fresh()?;
        if self.permission != AutomationPermission::Execute {
            return Err(ComputerError::Forbidden);
        }
        Ok(self.limits)
    }
    pub fn permission(&self) -> AutomationPermission {
        self.permission
    }
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
    pub fn valid_until(&self) -> Instant {
        self.deadline
    }
    fn check_fresh(&self) -> Result<()> {
        if Instant::now() >= self.deadline {
            return Err(ComputerError::Forbidden);
        }
        Ok(())
    }
}

pub(super) struct OwnerAuthority {
    pub snapshot: AuthoritySnapshot,
    expires_at: DateTime<Utc>,
}
impl OwnerAuthority {
    pub fn check_fresh(&self, actor: &ComputerActor) -> Result<()> {
        self.snapshot.check_fresh()?;
        actor.check_admission()?;
        if self.expires_at <= Utc::now() {
            return Err(ComputerError::Forbidden);
        }
        Ok(())
    }
    pub fn bindings(&self, actor: &ComputerActor) -> Result<Vec<(&'static str, Value)>> {
        self.check_fresh(actor)?;
        let family = actor
            .accepted()
            .request_context
            .access_token
            .session_family
            .as_ref()
            .map(|id| {
                id.as_str()
                    .parse::<Uuid>()
                    .map(gateway_refresh_family_record_id)
            })
            .transpose()
            .map_err(|_| ComputerError::Forbidden)?;
        let mut params = crate::session_grants::authority::bindings(&self.snapshot);
        params.extend([
            ("family", family.into_value()),
            ("authority_expires_at", self.expires_at.into_value()),
        ]);
        Ok(params)
    }
}
pub(super) fn owned(
    grant: &super::model::Grant,
    actor: &ComputerActor,
    computer: Uuid,
    provider: Uuid,
) -> Result<()> {
    if grant.view.computer_id != computer
        || grant.owner_key != owner_key(actor.owner())?
        || grant.provider != provider
    {
        return Err(ComputerError::NotFound);
    }
    Ok(())
}

fn require_tool(snapshot: &AuthoritySnapshot, tool: &str) -> Result<()> {
    snapshot.check_fresh()?;
    let target = PolicyTarget::Tool {
        server: ServerSlug::new("computers").expect("static server"),
        tool: LocalToolName::new(tool).expect("static tool"),
    };
    let trace = TraceId::new(Uuid::now_v7().to_string()).expect("UUID trace");
    if !snapshot
        .membership
        .allows(WorkContextMembershipLevel::Contributor)
        || snapshot
            .decision(GatewayAction::ToolsCall, &target, &trace)
            .effect
            != PolicyEffect::Allow
    {
        return Err(ComputerError::Forbidden);
    }
    Ok(())
}
pub(super) fn require_permission(
    snapshot: &AuthoritySnapshot,
    computer: Uuid,
    permission: AutomationPermission,
) -> Result<()> {
    if permission != AutomationPermission::Read {
        return require_tool(snapshot, super::model::permission_name(permission));
    }
    snapshot.check_fresh()?;
    let target = PolicyTarget::Resource {
        server: ServerSlug::new("computers").expect("static server"),
        uri: ResourceUri::new(crate::api::computer_uri(computer)).expect("Computer URI"),
    };
    let trace = TraceId::new(Uuid::now_v7().to_string()).expect("UUID trace");
    if snapshot
        .decision(GatewayAction::ResourcesRead, &target, &trace)
        .effect
        != PolicyEffect::Allow
    {
        return Err(ComputerError::Forbidden);
    }
    Ok(())
}

impl ComputersStore {
    pub(super) async fn automation_owner(
        &self,
        actor: &ComputerActor,
        tool: &str,
    ) -> Result<OwnerAuthority> {
        actor.check_admission()?;
        // A grant cannot delegate the power to issue another grant. Ownership and
        // the verified source principal must coincide at this management boundary.
        if actor.accepted().actor.id != actor.accepted().request_context.principal.id {
            return Err(ComputerError::Forbidden);
        }
        let snapshot = self.read_authority(actor.accepted()).await?;
        require_tool(&snapshot, tool)?;
        let family_expiry = self.check_control_session(&snapshot).await?;
        let expires_at = actor
            .admission_expires_at()
            .min(snapshot.checked_at + TimeDelta::seconds(30));
        Ok(OwnerAuthority {
            snapshot,
            expires_at: family_expiry.map_or(expires_at, |expiry| expiry.min(expires_at)),
        })
    }

    pub async fn authorize_automation_grant(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
        grant_id: Uuid,
        permission: AutomationPermission,
    ) -> Result<AutomationAuthority> {
        tokio::time::timeout(
            Duration::from_secs(5),
            self.read_automation_authority(actor, computer_id, grant_id, permission),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    async fn read_automation_authority(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
        grant_id: Uuid,
        permission: AutomationPermission,
    ) -> Result<AutomationAuthority> {
        actor.check_admission()?;
        let grant = self.automation_grant(grant_id).await?;
        let source = &actor.accepted().request_context.principal;
        let kind = match source.kind {
            veoveo_mcp_contract::PrincipalKind::User => PrincipalKind::User,
            veoveo_mcp_contract::PrincipalKind::Service => PrincipalKind::Service,
        };
        if grant.view.computer_id != computer_id
            || grant.provider != self.provider_instance_id
            || grant.view.principal_id != source.id.as_str()
            || grant.view.oauth_client_id
                != actor
                    .accepted()
                    .request_context
                    .access_token
                    .oauth_client_id
                    .as_str()
            || grant.grantee_issuer != source.issuer.as_str()
            || grant.grantee_subject != source.subject.as_str()
            || grant.grantee_kind != kind
            || grant.authority.invocation.tenant != actor.accepted().invocation.tenant
            || grant.authority.invocation.work_context != actor.accepted().invocation.work_context
            || grant.authority.profile != actor.accepted().profile
        {
            return Err(ComputerError::NotFound);
        }
        if grant.view.revoked_at.is_some()
            || grant.view.expires_at <= Utc::now()
            || !grant.view.permissions.contains(&permission)
        {
            return Err(ComputerError::Forbidden);
        }
        let policy = self.stored_automation_policy().await?.checked()?;
        if policy.max_grants == 0 {
            return Err(ComputerError::Forbidden);
        }
        let source_snapshot = self.read_authority(actor.accepted()).await?;
        let family_expiry = self.check_control_session(&source_snapshot).await?;
        let owner_snapshot = self.read_authority(&grant.authority).await?;
        if source_snapshot.source != grant.grantee
            || source_snapshot.control_sha256 != owner_snapshot.control_sha256
        {
            return Err(ComputerError::Forbidden);
        }
        require_permission(&source_snapshot, computer_id, permission)?;
        require_permission(&owner_snapshot, computer_id, permission)?;
        let mut read = self
            .query(
                "SELECT * FROM ONLY $computer;",
                vec![("computer", computer_record(computer_id).into_value())],
            )
            .await?;
        let row: Option<ComputerRecord> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let computer = Computer::try_from(row.ok_or(ComputerError::NotFound)?)?;
        if computer.provider_instance_id != grant.provider
            || owner_key(&computer.owner)? != grant.owner_key
            || !computer.owner.data_labels.iter().all(|label| {
                source
                    .data_labels
                    .iter()
                    .any(|actual| actual.as_str() == label)
            })
            || !computer
                .owner
                .authority
                .output_policy
                .data_labels
                .is_subset(&source.data_labels)
            || computer
                .owner
                .authority
                .output_policy
                .classification
                .as_ref()
                .is_some_and(|label| !source.data_labels.contains(label))
        {
            return Err(ComputerError::Forbidden);
        }
        let limits = grant
            .view
            .execution_limits
            .map(|limits| AutomationExecutionLimits {
                maximum_seconds: limits.maximum_seconds.min(policy.maximum_execution_seconds),
                maximum_output_bytes: limits.maximum_output_bytes.min(policy.maximum_output_bytes),
            });
        let expires_at = grant.view.expires_at.min(
            grant.view.issued_at + TimeDelta::seconds(i64::from(policy.maximum_lifetime_seconds)),
        );
        let admission_end = expires_at.min(actor.admission_expires_at());
        let admission_end = family_expiry.map_or(admission_end, |expiry| expiry.min(admission_end));
        let remaining = (admission_end - Utc::now())
            .to_std()
            .map_err(|_| ComputerError::Forbidden)?;
        if remaining.is_zero() {
            return Err(ComputerError::Forbidden);
        }
        let deadline = source_snapshot
            .deadline
            .min(owner_snapshot.deadline)
            .min(Instant::now() + remaining);
        actor.check_admission()?;
        source_snapshot.check_fresh()?;
        owner_snapshot.check_fresh()?;
        Ok(AutomationAuthority {
            computer,
            grant_id,
            grant_revision: grant.revision,
            permission,
            limits,
            deadline,
            expires_at,
        })
    }
}
