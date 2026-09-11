use super::{FileOperation, FileTransferStage};
use crate::{
    ComputerError, ComputersStore, Result,
    api::FileTransferLimits,
    secrets::{ComputerKeyRing, FileTransferAccess, FileTransferBinding, FileTransferPayload},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_mcp_contract::{GatewayAction, PolicyDecision, PolicyEffect};
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileDispatchDecision {
    pub control_revision: String,
    pub control_sha256: String,
    pub grant_id: Option<Uuid>,
    pub grant_revision: Option<u64>,
    pub checked_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub source: PolicyDecision,
    pub owner: Option<PolicyDecision>,
}
impl FileDispatchDecision {
    pub(super) fn validate(&self, operation: &FileOperation) -> Result<()> {
        if self.grant_id != operation.binding.grant_id
            || self.control_revision.is_empty()
            || self.control_sha256.len() != 64
            || !self.control_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || self.valid_until <= self.checked_at
            || self.valid_until - self.checked_at > chrono::TimeDelta::seconds(30)
            || self.source.principal.as_ref()
                != Some(&operation.authority.request_context.principal.id)
        {
            return Err(ComputerError::Unavailable);
        }
        if self.grant_id.is_some() != self.grant_revision.is_some()
            || self.grant_id.is_some() != self.owner.is_some()
        {
            return Err(ComputerError::Unavailable);
        }
        for decision in std::iter::once(&self.source).chain(self.owner.iter()) {
            if decision.effect != PolicyEffect::Allow
                || decision.action != GatewayAction::ToolsCall
                || decision.profile != operation.authority.profile
                || decision.tenant.as_ref() != Some(&operation.authority.invocation.tenant)
                || decision.trace_id.as_str() != operation.transfer_id().to_string()
                || decision.target != super::target()
            {
                return Err(ComputerError::Unavailable);
            }
        }
        Ok(())
    }
}
/// Only one committed queued-to-dispatched transition can return this value.
/// Losing it permits containment/recovery, never a second file dispatch.
pub struct FileDispatchTicket {
    operation: FileOperation,
    payload: FileTransferPayload,
    access: FileTransferAccess,
    authority_deadline: Instant,
    execution_deadline: Instant,
}
impl FileDispatchTicket {
    pub(super) fn into_operation(self) -> FileOperation {
        self.operation
    }
    pub fn operation(&self) -> &FileOperation {
        &self.operation
    }
    pub fn binding(&self) -> &FileTransferBinding {
        &self.operation.binding
    }
    pub fn payload(&self) -> &FileTransferPayload {
        &self.payload
    }
    pub fn access(&self) -> &FileTransferAccess {
        &self.access
    }
    pub fn limits(&self) -> FileTransferLimits {
        self.operation
            .effective_limits
            .expect("validated dispatched limits")
    }
    pub fn authority_deadline(&self) -> Instant {
        self.authority_deadline
    }
    pub fn execution_deadline(&self) -> Instant {
        self.execution_deadline
    }
}

impl ComputersStore {
    pub async fn begin_file_dispatch(
        &self,
        claim: &ClaimedTask,
        keys: &ComputerKeyRing,
    ) -> Result<FileDispatchTicket> {
        tokio::time::timeout(Duration::from_secs(10), self.dispatch_file(claim, keys))
            .await
            .map_err(|_| ComputerError::Unavailable)?
    }
    async fn dispatch_file(
        &self,
        claim: &ClaimedTask,
        keys: &ComputerKeyRing,
    ) -> Result<FileDispatchTicket> {
        let mut operation = self.worker_file(claim).await?.operation;
        if operation.stage != FileTransferStage::Queued {
            return Err(ComputerError::StateConflict);
        }
        let permit = self.accepted_file_authority(&operation).await?;
        let payload = keys.open_file_transfer(&operation.binding, &operation.sealed)?;
        let sealed_access = operation
            .access
            .as_ref()
            .ok_or(ComputerError::InvalidState)?;
        let access = keys.open_file_access(&operation.binding, sealed_access)?;
        let limits = permit.limits()?;
        let effective = FileTransferLimits {
            maximum_seconds: payload.limits().maximum_seconds.min(limits.maximum_seconds),
            maximum_bytes: payload.limits().maximum_bytes.min(limits.maximum_bytes),
            on_interruption: limits.on_interruption,
        };
        let current = permit.computer()?;
        if current.phase != crate::api::ComputerPhase::Ready {
            return Err(ComputerError::InvalidState);
        }
        if current.active_operation.is_some() {
            return Err(ComputerError::OperationBusy);
        }
        if current.provider_instance_id != operation.binding.provider_instance_id
            || crate::identity::owner_key(&current.owner)? != operation.binding.owner_key
            || current
                .replacement_instance_id
                .unwrap_or(current.computer_id)
                != operation.binding.instance_id
            || current.template_fingerprint != operation.binding.template_fingerprint
            || current.provider_resource_id.as_deref() != Some(&operation.binding.resource_id)
            || current.process_id.as_deref() != Some(&operation.binding.process_id)
        {
            return Err(ComputerError::StateConflict);
        }
        let decision = permit.decision(operation.transfer_id())?;
        decision.validate(&operation)?;
        let as_object = |value: serde_json::Value| -> Result<veoveo_platform_store::OpenObject> {
            serde_json::from_value(value).map_err(|_| ComputerError::Unavailable)
        };
        let evidence =
            as_object(serde_json::to_value(&decision).map_err(|_| ComputerError::Unavailable)?)?;
        operation.dispatch_authority = Some(decision);
        let id = Uuid::now_v7();
        let mut params = permit.transaction_bindings()?;
        params.extend([
            ("policy", self.automation_policy_record().into_value()),
            ("execution_expiry", permit.execution_expiry().into_value()),
            ("dispatch_id", id.into_value()),
            (
                "preparation_budget",
                surrealdb::types::Duration::from_secs(u64::from(super::FILE_PREPARATION_SECONDS))
                    .into_value(),
            ),
            (
                "expected_access",
                as_object(
                    serde_json::to_value(sealed_access).map_err(|_| ComputerError::Unavailable)?,
                )?
                .into_value(),
            ),
            ("access_expires", access.expires_at().into_value()),
            (
                "publication_budget",
                surrealdb::types::Duration::from_secs(u64::from(super::FILE_PUBLICATION_SECONDS))
                    .into_value(),
            ),
            (
                "expected_binding",
                as_object(
                    serde_json::to_value(&operation.binding)
                        .map_err(|_| ComputerError::Unavailable)?,
                )?
                .into_value(),
            ),
            ("decision", evidence.into_value()),
            (
                "effective_limits",
                as_object(
                    serde_json::to_value(effective).map_err(|_| ComputerError::Unavailable)?,
                )?
                .into_value(),
            ),
            (
                "duration",
                surrealdb::types::Duration::from_secs(u64::from(effective.maximum_seconds))
                    .into_value(),
            ),
            (
                "expected_owner_context",
                as_object(
                    serde_json::to_value(&current.owner).map_err(|_| ComputerError::Unavailable)?,
                )?
                .into_value(),
            ),
        ]);
        self.commit_file(
            claim,
            &operation,
            ProviderCommit::Dispatch,
            include_str!("../../queries/dispatch_file.surql"),
            params,
            "computer.file_transfer_dispatched",
        )
        .await?;
        let selected = self.worker_file(claim).await?;
        let deadline = selected
            .operation
            .execution_deadline
            .ok_or(ComputerError::Unavailable)?;
        let remaining = (deadline - selected.database_time)
            .to_std()
            .unwrap_or_default()
            .saturating_sub(selected.read_started.elapsed());
        let lease_remaining = (claim.lease_expires_at - selected.database_time)
            .to_std()
            .unwrap_or_default()
            .saturating_sub(selected.read_started.elapsed());
        // Start the short live-authority window at the actual dispatch commit,
        // never at receipt delivery after a delayed database response.
        let authority_remaining = selected.remaining(selected.operation.dispatched_at.map(|at| {
            at + chrono::TimeDelta::seconds(super::continuation::AUTHORITY_WINDOW.as_secs() as i64)
        }));
        if selected.operation.dispatch_id != Some(id)
            || selected.operation.stage != FileTransferStage::Dispatched
            || remaining.is_zero()
            || lease_remaining.is_zero()
            || authority_remaining.is_zero()
            || permit.valid_until() <= Instant::now()
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(FileDispatchTicket {
            operation: selected.operation,
            payload,
            access,
            authority_deadline: permit
                .valid_until()
                .min(Instant::now() + lease_remaining.min(authority_remaining)),
            execution_deadline: Instant::now() + remaining,
        })
    }
}
