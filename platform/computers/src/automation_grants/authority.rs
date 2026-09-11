use crate::{
    AcceptedAuthority, Computer, ComputerActor, ComputerError, ComputersStore, Result,
    api::{AutomationExecutionLimits, AutomationPermission},
    authority_snapshot::AuthoritySnapshot,
    identity::owner_key,
    model::{ComputerRecord, computer_record},
};
use chrono::{DateTime, TimeDelta, Utc};
use std::time::{Duration, Instant};
use surrealdb::types::{RecordId, SurrealValue, Value};
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayAction, LocalToolName, PolicyEffect, PolicyTarget, ResourceUri, ServerSlug, TraceId,
    WorkContextMembershipLevel,
};
use veoveo_platform_store::{PrincipalKind, gateway_refresh_family_record_id};

#[derive(Clone, Copy)]
enum GrantUse {
    Admission(DateTime<Utc>),
    AcceptedWork,
}

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
    source_snapshot: AuthoritySnapshot,
    owner_snapshot: AuthoritySnapshot,
    policy_fingerprint: String,
    admission_end: DateTime<Utc>,
    family: Option<RecordId>,
}
impl AutomationAuthority {
    pub(crate) fn file_decision(
        &self,
        transfer: Uuid,
    ) -> Result<crate::files::FileDispatchDecision> {
        self.require_file_transfer()?;
        let trace = TraceId::new(transfer.to_string()).expect("UUID trace");
        Ok(crate::files::FileDispatchDecision {
            control_revision: self.source_snapshot.control_revision.clone(),
            control_sha256: self.source_snapshot.control_sha256.clone(),
            grant_id: Some(self.grant_id),
            grant_revision: Some(self.grant_revision),
            checked_at: self
                .source_snapshot
                .checked_at
                .max(self.owner_snapshot.checked_at),
            valid_until: self
                .admission_end
                .min(self.source_snapshot.checked_at + TimeDelta::seconds(30))
                .min(self.owner_snapshot.checked_at + TimeDelta::seconds(30)),
            source: self.source_snapshot.decision(
                GatewayAction::ToolsCall,
                &crate::files::target(),
                &trace,
            ),
            owner: Some(self.owner_snapshot.decision(
                GatewayAction::ToolsCall,
                &crate::files::target(),
                &trace,
            )),
        })
    }
    pub(crate) fn require_file_transfer(&self) -> Result<()> {
        self.check_fresh()?;
        if self.permission != AutomationPermission::Execute {
            return Err(ComputerError::Forbidden);
        }
        require_tool(&self.source_snapshot, "transfer_file")?;
        require_tool(&self.owner_snapshot, "transfer_file")
    }
    pub(crate) fn command_decision(
        &self,
        execution: Uuid,
    ) -> Result<crate::commands::CommandDispatchDecision> {
        self.check_fresh()?;
        if self.permission != AutomationPermission::Execute {
            return Err(ComputerError::Forbidden);
        }
        let target = PolicyTarget::Tool {
            server: ServerSlug::new("computers").expect("static server"),
            tool: LocalToolName::new("execute").expect("static tool"),
        };
        let trace = TraceId::new(execution.to_string()).expect("UUID trace");
        Ok(crate::commands::CommandDispatchDecision {
            control_revision: self.source_snapshot.control_revision.clone(),
            control_sha256: self.source_snapshot.control_sha256.clone(),
            grant_id: self.grant_id,
            grant_revision: self.grant_revision,
            checked_at: self
                .source_snapshot
                .checked_at
                .max(self.owner_snapshot.checked_at),
            valid_until: self
                .admission_end
                .min(self.source_snapshot.checked_at + TimeDelta::seconds(30))
                .min(self.owner_snapshot.checked_at + TimeDelta::seconds(30)),
            source: self
                .source_snapshot
                .decision(GatewayAction::ToolsCall, &target, &trace),
            owner: self
                .owner_snapshot
                .decision(GatewayAction::ToolsCall, &target, &trace),
        })
    }
    pub(crate) fn require_actor(&self, actor: &ComputerActor) -> Result<()> {
        self.check_fresh()?;
        actor.check_admission()?;
        if crate::identity::digest(&self.source_snapshot.accepted)?
            != crate::identity::digest(actor.accepted())?
        {
            return Err(ComputerError::Forbidden);
        }
        Ok(())
    }
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
    pub(crate) fn transaction_bindings(&self) -> Result<Vec<(&'static str, Value)>> {
        self.check_fresh()?;
        self.source_snapshot.check_fresh()?;
        self.owner_snapshot.check_fresh()?;
        let mut bindings = crate::session_grants::authority::bindings(&self.source_snapshot);
        bindings.extend([
            (
                "authority_owner_source",
                self.owner_snapshot.source.clone().into_value(),
            ),
            (
                "authority_owner_actor",
                self.owner_snapshot.actor.clone().into_value(),
            ),
            (
                "authority_expires_at",
                self.admission_end
                    .min(self.source_snapshot.checked_at + TimeDelta::seconds(30))
                    .min(self.owner_snapshot.checked_at + TimeDelta::seconds(30))
                    .into_value(),
            ),
            ("family", self.family.clone().into_value()),
            ("grant", super::record(self.grant_id).into_value()),
            ("grant_revision", self.grant_revision.into_value()),
            ("grant_expires_at", self.expires_at.into_value()),
            (
                "policy_fingerprint",
                self.policy_fingerprint.clone().into_value(),
            ),
        ]);
        Ok(bindings)
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
    /// Current UI hints. Every mutation independently rechecks its authority.
    pub(super) async fn automation_management(
        &self,
        actor: &ComputerActor,
    ) -> Result<(bool, bool)> {
        actor.check_admission()?;
        if actor.accepted().actor.id != actor.accepted().request_context.principal.id {
            return Ok((false, false));
        }
        let snapshot = self.read_authority(actor.accepted()).await?;
        self.check_control_session(&snapshot).await?;
        let grant = require_tool(&snapshot, "grant_automation").is_ok();
        let revoke = require_tool(&snapshot, "revoke_automation").is_ok();
        snapshot.check_fresh()?;
        actor.check_admission()?;
        Ok((grant, revoke))
    }
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
        actor.check_admission()?;
        tokio::time::timeout(
            Duration::from_secs(5),
            self.read_automation_authority(
                actor.accepted(),
                GrantUse::Admission(actor.admission_expires_at()),
                computer_id,
                grant_id,
                permission,
            ),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    pub(crate) async fn authorize_accepted_automation(
        &self,
        accepted: &AcceptedAuthority,
        computer: Uuid,
        grant: Uuid,
        permission: AutomationPermission,
    ) -> Result<AutomationAuthority> {
        tokio::time::timeout(
            Duration::from_secs(5),
            self.read_automation_authority(
                accepted,
                GrantUse::AcceptedWork,
                computer,
                grant,
                permission,
            ),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    async fn read_automation_authority(
        &self,
        accepted: &AcceptedAuthority,
        purpose: GrantUse,
        computer_id: Uuid,
        grant_id: Uuid,
        permission: AutomationPermission,
    ) -> Result<AutomationAuthority> {
        if let GrantUse::Admission(expires_at) = purpose
            && expires_at <= Utc::now()
        {
            return Err(ComputerError::Forbidden);
        }
        let grant = self.automation_grant(grant_id).await?;
        let source = &accepted.request_context.principal;
        let kind = match source.kind {
            veoveo_mcp_contract::PrincipalKind::User => PrincipalKind::User,
            veoveo_mcp_contract::PrincipalKind::Service => PrincipalKind::Service,
        };
        if grant.view.computer_id != computer_id
            || grant.provider != self.provider_instance_id
            || grant.view.principal_id != source.id.as_str()
            || grant.view.oauth_client_id
                != accepted
                    .request_context
                    .access_token
                    .oauth_client_id
                    .as_str()
            || grant.grantee_issuer != source.issuer.as_str()
            || grant.grantee_subject != source.subject.as_str()
            || grant.grantee_kind != kind
            || grant.authority.invocation.tenant != accepted.invocation.tenant
            || grant.authority.invocation.work_context != accepted.invocation.work_context
            || grant.authority.profile != accepted.profile
        {
            return Err(ComputerError::NotFound);
        }
        if grant.view.revoked_at.is_some()
            || grant.view.expires_at <= Utc::now()
            || !grant.view.permissions.contains(&permission)
        {
            return Err(ComputerError::Forbidden);
        }
        let stored_policy = self.stored_automation_policy().await?;
        let policy = stored_policy.checked()?;
        if policy.max_grants == 0 {
            return Err(ComputerError::Forbidden);
        }
        let source_snapshot = self.read_authority(accepted).await?;
        let (family_expiry, family) = match purpose {
            GrantUse::AcceptedWork => (None, None),
            GrantUse::Admission(_) => {
                let expiry = self.check_control_session(&source_snapshot).await?;
                let family = accepted
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
                (expiry, family)
            }
        };
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
                on_interruption: limits.on_interruption,
            });
        let expires_at = grant.view.expires_at.min(
            grant.view.issued_at + TimeDelta::seconds(i64::from(policy.maximum_lifetime_seconds)),
        );
        let admission_end = match purpose {
            GrantUse::Admission(admission) => expires_at.min(admission),
            GrantUse::AcceptedWork => expires_at,
        };
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
        if let GrantUse::Admission(expires_at) = purpose
            && expires_at <= Utc::now()
        {
            return Err(ComputerError::Forbidden);
        }
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
            source_snapshot,
            owner_snapshot,
            policy_fingerprint: stored_policy.fingerprint,
            admission_end,
            family,
        })
    }
}
