//! Short current authority. A successful read does not authorize provider dispatch.
use crate::{
    Computer, ComputerActor, ComputerError, ComputersStore, Result,
    api::{AutomationPermission, FileTransferLimits, MAX_TRANSFER_BYTES},
    authority_snapshot::AuthoritySnapshot,
    automation_grants::AutomationAuthority,
};
use chrono::{DateTime, Utc};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};
use surrealdb::types::{RecordId, SurrealValue, Value};
use uuid::Uuid;
use veoveo_mcp_contract::{
    DataLabelId, GatewayAction, PolicyEffect, TraceId, WorkContextMembershipLevel,
};

struct Owned {
    computer: Computer,
    snapshot: AuthoritySnapshot,
    expires_at: DateTime<Utc>,
    deadline: Instant,
    family: Option<RecordId>,
}
enum Authority {
    Owned(Box<Owned>),
    Delegated(Box<AutomationAuthority>),
}

pub struct FileTransferAuthority(Authority);
impl FileTransferAuthority {
    pub(super) fn execution_expiry(&self) -> Option<DateTime<Utc>> {
        match &self.0 {
            Authority::Owned(_) => None,
            Authority::Delegated(value) => Some(value.expires_at()),
        }
    }
    pub(super) fn decision(&self, transfer: Uuid) -> Result<super::FileDispatchDecision> {
        self.computer()?;
        match &self.0 {
            Authority::Delegated(value) => value.file_decision(transfer),
            Authority::Owned(value) => Ok(super::FileDispatchDecision {
                control_revision: value.snapshot.control_revision.clone(),
                control_sha256: value.snapshot.control_sha256.clone(),
                grant_id: None,
                grant_revision: None,
                checked_at: value.snapshot.checked_at,
                valid_until: value
                    .expires_at
                    .min(value.snapshot.checked_at + chrono::TimeDelta::seconds(30)),
                source: value.snapshot.decision(
                    GatewayAction::ToolsCall,
                    &super::target(),
                    &TraceId::new(transfer.to_string()).expect("UUID trace"),
                ),
                owner: None,
            }),
        }
    }
    pub fn computer(&self) -> Result<&Computer> {
        match &self.0 {
            Authority::Owned(value) => {
                value.snapshot.check_fresh()?;
                if value.expires_at <= Utc::now() || value.deadline <= Instant::now() {
                    return Err(ComputerError::Forbidden);
                }
                Ok(&value.computer)
            }
            Authority::Delegated(value) => value.computer(),
        }
    }
    pub fn grant_id(&self) -> Option<Uuid> {
        match &self.0 {
            Authority::Owned(_) => None,
            Authority::Delegated(value) => Some(value.grant_id()),
        }
    }
    pub fn valid_until(&self) -> Instant {
        match &self.0 {
            Authority::Owned(value) => value.deadline,
            Authority::Delegated(value) => value.valid_until(),
        }
    }
    pub fn limits(&self) -> Result<FileTransferLimits> {
        self.computer()?;
        let limits = match &self.0 {
            Authority::Owned(_) => FileTransferLimits {
                maximum_seconds: 300,
                maximum_bytes: MAX_TRANSFER_BYTES,
                on_interruption: crate::api::AutomationInterruption::StopComputer,
            },
            Authority::Delegated(value) => {
                let limits = value.execution_limits()?.ok_or(ComputerError::Forbidden)?;
                FileTransferLimits {
                    maximum_seconds: limits.maximum_seconds.min(300),
                    maximum_bytes: u64::from(limits.maximum_output_bytes).min(MAX_TRANSFER_BYTES),
                    on_interruption: limits.on_interruption,
                }
            }
        };
        Ok(limits)
    }
    pub fn required_labels(&self, actor: &ComputerActor) -> Result<BTreeSet<DataLabelId>> {
        self.require_actor(actor)?;
        let mut labels = self.retained_labels()?;
        labels.extend(
            actor
                .owner()
                .authority
                .output_policy
                .data_labels
                .iter()
                .cloned(),
        );
        labels.extend(
            actor
                .owner()
                .authority
                .output_policy
                .classification
                .iter()
                .cloned(),
        );
        Ok(labels)
    }
    pub(super) fn retained_labels(&self) -> Result<BTreeSet<DataLabelId>> {
        let computer = self.computer()?;
        let mut labels = computer
            .owner
            .data_labels
            .iter()
            .map(|value| DataLabelId::new(value.clone()).map_err(|_| ComputerError::Unavailable))
            .collect::<Result<BTreeSet<_>>>()?;
        let policy = &computer.owner.authority.output_policy;
        labels.extend(policy.data_labels.iter().cloned());
        labels.extend(policy.classification.iter().cloned());
        Ok(labels)
    }
    pub(crate) fn require_actor(&self, actor: &ComputerActor) -> Result<()> {
        actor.check_admission()?;
        self.computer()?;
        match &self.0 {
            Authority::Owned(value)
                if crate::identity::digest(&value.snapshot.accepted)?
                    != crate::identity::digest(actor.accepted())? =>
            {
                Err(ComputerError::Forbidden)
            }
            Authority::Owned(_) => Ok(()),
            Authority::Delegated(value) => value.require_actor(actor),
        }
    }
    pub(crate) fn transaction_bindings(&self) -> Result<Vec<(&'static str, Value)>> {
        self.computer()?;
        match &self.0 {
            Authority::Delegated(value) => value.transaction_bindings(),
            Authority::Owned(value) => {
                let mut params = crate::session_grants::authority::bindings(&value.snapshot);
                params.extend([
                    (
                        "authority_expires_at",
                        value
                            .expires_at
                            .min(value.snapshot.checked_at + chrono::TimeDelta::seconds(30))
                            .into_value(),
                    ),
                    ("family", value.family.clone().into_value()),
                    ("grant", Option::<RecordId>::None.into_value()),
                ]);
                Ok(params)
            }
        }
    }
}

impl ComputersStore {
    /// Accepted work uses current directory and action policy independently of
    /// the original sign-in token. A delegated transfer still requires its grant.
    pub(super) async fn accepted_file_authority(
        &self,
        operation: &super::FileOperation,
    ) -> Result<FileTransferAuthority> {
        let accepted = &operation.authority;
        if let Some(grant) = operation.binding.grant_id {
            let authority = self
                .authorize_accepted_automation(
                    accepted,
                    operation.computer_id(),
                    grant,
                    AutomationPermission::Execute,
                )
                .await?;
            authority.require_file_transfer()?;
            return Ok(FileTransferAuthority(Authority::Delegated(Box::new(
                authority,
            ))));
        }
        if accepted.actor.id != accepted.request_context.principal.id {
            return Err(ComputerError::Forbidden);
        }
        let snapshot = self.read_authority(accepted).await?;
        if !snapshot
            .membership
            .allows(WorkContextMembershipLevel::Contributor)
            || snapshot
                .decision(
                    GatewayAction::ToolsCall,
                    &super::target(),
                    &TraceId::new(operation.transfer_id().to_string()).expect("UUID trace"),
                )
                .effect
                != PolicyEffect::Allow
        {
            return Err(ComputerError::Forbidden);
        }
        let computer = self
            .get(&accepted.task_owner(), operation.computer_id())
            .await?;
        if computer.provider_instance_id != self.provider_instance_id {
            return Err(ComputerError::NotFound);
        }
        snapshot.check_fresh()?;
        let expires_at = snapshot.checked_at + chrono::TimeDelta::seconds(30);
        let deadline = snapshot.deadline;
        Ok(FileTransferAuthority(Authority::Owned(Box::new(Owned {
            computer,
            snapshot,
            expires_at,
            deadline,
            family: None,
        }))))
    }
    pub async fn file_transfer_authority(
        &self,
        actor: &ComputerActor,
        computer: Uuid,
        grant: Option<Uuid>,
    ) -> Result<FileTransferAuthority> {
        actor.check_admission()?;
        let authority = tokio::time::timeout(Duration::from_secs(5), async {
            if let Some(grant) = grant {
                let authority = self
                    .authorize_automation_grant(
                        actor,
                        computer,
                        grant,
                        AutomationPermission::Execute,
                    )
                    .await?;
                authority.require_file_transfer()?;
                return Ok(FileTransferAuthority(Authority::Delegated(Box::new(
                    authority,
                ))));
            }
            if actor.accepted().actor.id != actor.accepted().request_context.principal.id {
                return Err(ComputerError::Forbidden);
            }
            let snapshot = self.read_authority(actor.accepted()).await?;
            let session_expiry = self.check_control_session(&snapshot).await?;
            snapshot.check_fresh()?;
            if !snapshot
                .membership
                .allows(WorkContextMembershipLevel::Contributor)
                || snapshot
                    .decision(
                        GatewayAction::ToolsCall,
                        &super::target(),
                        &TraceId::new(Uuid::now_v7().to_string()).expect("UUID trace"),
                    )
                    .effect
                    != PolicyEffect::Allow
            {
                return Err(ComputerError::Forbidden);
            }
            let computer = self.get(actor.owner(), computer).await?;
            if computer.provider_instance_id != self.provider_instance_id {
                return Err(ComputerError::NotFound);
            }
            let family = actor
                .accepted()
                .request_context
                .access_token
                .session_family
                .as_ref()
                .map(|id| {
                    id.as_str()
                        .parse::<Uuid>()
                        .map(veoveo_platform_store::gateway_refresh_family_record_id)
                })
                .transpose()
                .map_err(|_| ComputerError::Forbidden)?;
            let expires_at = session_expiry
                .unwrap_or(actor.admission_expires_at())
                .min(actor.admission_expires_at());
            let remaining = (expires_at - Utc::now())
                .to_std()
                .map_err(|_| ComputerError::Forbidden)?;
            let deadline = snapshot.deadline.min(Instant::now() + remaining);
            Ok(FileTransferAuthority(Authority::Owned(Box::new(Owned {
                computer,
                snapshot,
                expires_at,
                deadline,
                family,
            }))))
        })
        .await
        .map_err(|_| ComputerError::Unavailable)??;
        authority.require_actor(actor)?;
        Ok(authority)
    }
}
