use crate::{
    AcceptedAuthority, ComputerError, Result,
    secrets::{FileTransferBinding, SealedFileTransfer, SealedFileTransferAccess},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::{TaskId, TaskOwner};

use crate::api::FileTransferStage;

/// Private durable file; public projections must select metadata explicitly.
pub struct FileOperation {
    pub(super) binding: FileTransferBinding,
    pub(super) authority: AcceptedAuthority,
    pub(super) sealed: SealedFileTransfer,
    pub(super) access: Option<SealedFileTransferAccess>,
    pub(super) created_at: DateTime<Utc>,
    pub(super) stage: FileTransferStage,
    pub(super) dispatch_id: Option<Uuid>,
    pub(super) dispatched_at: Option<DateTime<Utc>>,
    pub(super) execution_deadline: Option<DateTime<Utc>>,
    pub(super) effective_limits: Option<crate::api::FileTransferLimits>,
    pub(super) dispatch_authority: Option<super::FileDispatchDecision>,
    pub(super) containment_id: Option<Uuid>,
    pub(super) interruption: Option<super::FileInterruption>,
    pub(super) containment_dispatch_id: Option<Uuid>,
    pub(super) containment_started_at: Option<DateTime<Utc>>,
    pub(super) containment_deadline: Option<DateTime<Utc>>,
    pub(super) containment_reads: u32,
    pub(super) next_containment_read: Option<DateTime<Utc>>,
    pub(super) last_containment_read_id: Option<Uuid>,
    pub(super) settled_at: Option<DateTime<Utc>>,
    pub(super) refusal: Option<super::FileRefusal>,
    pub(super) terminated_at: Option<DateTime<Utc>>,
    pub(super) termination_evidence: Option<super::outcome::TerminationEvidence>,
    pub(super) task_projected_at: Option<DateTime<Utc>>,
    pub(super) result: Option<crate::api::FileTransferResult>,
    pub(super) rejection: Option<veoveo_computer_execution::FileFailure>,
}
impl FileOperation {
    pub fn binding(&self) -> &FileTransferBinding {
        &self.binding
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.stage,
            FileTransferStage::Failed | FileTransferStage::Cancelled | FileTransferStage::Completed
        )
    }
    pub fn outcome(&self) -> Option<super::FileOutcome> {
        if !self.is_terminal() {
            return None;
        }
        if let Some(result) = &self.result {
            return Some(super::FileOutcome::Completed(result.clone()));
        }
        if let Some(reason) = self.rejection {
            return Some(super::FileOutcome::Rejected(reason));
        }
        self.refusal
            .map(super::FileOutcome::Undispatched)
            .or_else(|| self.interruption.map(super::FileOutcome::Terminated))
    }
    pub fn containment_id(&self) -> Option<Uuid> {
        self.containment_id
    }
    pub fn task_projected(&self) -> bool {
        self.task_projected_at.is_some()
    }
    pub fn transfer_id(&self) -> Uuid {
        self.binding.transfer_id
    }
    pub fn stage(&self) -> FileTransferStage {
        self.stage
    }
    pub fn execution_deadline(&self) -> Option<DateTime<Utc>> {
        self.execution_deadline
    }
    pub fn computer_id(&self) -> Uuid {
        self.binding.computer_id
    }
    pub fn task_id(&self) -> TaskId {
        TaskId::from_uuid(self.transfer_id())
    }
    pub fn actor(&self) -> TaskOwner {
        self.authority.task_owner()
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub(super) fn task_reference(&self) -> Result<serde_json::Value> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Reference {
            computer_id: Uuid,
            transfer_id: Uuid,
        }
        serde_json::to_value(Reference {
            computer_id: self.computer_id(),
            transfer_id: self.transfer_id(),
        })
        .map_err(|_| ComputerError::Unavailable)
    }
}

#[derive(Deserialize, SurrealValue)]
pub(super) struct Record {
    id: RecordId,
    transfer_id: Uuid,
    computer_id: Uuid,
    provider_instance_id: Uuid,
    actor_key: String,
    owner_key: String,
    binding: OpenObject,
    authority: OpenObject,
    sealed: OpenObject,
    artifact_access: Option<OpenObject>,
    task: RecordId,
    stage: String,
    created_at: DateTime<Utc>,
    dispatch_id: Option<Uuid>,
    dispatched_at: Option<DateTime<Utc>>,
    execution_deadline: Option<DateTime<Utc>>,
    effective_limits: Option<OpenObject>,
    dispatch_authority: Option<OpenObject>,
    containment_id: Option<Uuid>,
    interruption: Option<String>,
    containment_dispatch_id: Option<Uuid>,
    containment_started_at: Option<DateTime<Utc>>,
    containment_deadline: Option<DateTime<Utc>>,
    containment_reads: u32,
    next_containment_read: Option<DateTime<Utc>>,
    last_containment_read_id: Option<Uuid>,
    settled_at: Option<DateTime<Utc>>,
    refusal: Option<String>,
    terminated_at: Option<DateTime<Utc>>,
    termination_evidence: Option<OpenObject>,
    task_projected_at: Option<DateTime<Utc>>,
    result: Option<OpenObject>,
    rejection: Option<String>,
}
impl TryFrom<Record> for FileOperation {
    type Error = ComputerError;
    fn try_from(row: Record) -> Result<Self> {
        let decode = || -> std::result::Result<_, serde_json::Error> {
            Ok(Self {
                binding: serde_json::from_value(serde_json::to_value(row.binding)?)?,
                authority: serde_json::from_value(serde_json::to_value(row.authority)?)?,
                sealed: serde_json::from_value(serde_json::to_value(row.sealed)?)?,
                access: row
                    .artifact_access
                    .map(|v| serde_json::from_value(serde_json::to_value(v)?))
                    .transpose()?,
                created_at: row.created_at,
                stage: serde_json::from_value(serde_json::Value::String(row.stage))?,
                dispatch_id: row.dispatch_id,
                dispatched_at: row.dispatched_at,
                execution_deadline: row.execution_deadline,
                effective_limits: row
                    .effective_limits
                    .map(|v| serde_json::from_value(serde_json::to_value(v)?))
                    .transpose()?,
                dispatch_authority: row
                    .dispatch_authority
                    .map(|v| serde_json::from_value(serde_json::to_value(v)?))
                    .transpose()?,
                containment_id: row.containment_id,
                interruption: row
                    .interruption
                    .map(|v| serde_json::from_value(serde_json::Value::String(v)))
                    .transpose()?,
                containment_dispatch_id: row.containment_dispatch_id,
                containment_started_at: row.containment_started_at,
                containment_deadline: row.containment_deadline,
                containment_reads: row.containment_reads,
                next_containment_read: row.next_containment_read,
                last_containment_read_id: row.last_containment_read_id,
                settled_at: row.settled_at,
                refusal: row
                    .refusal
                    .map(|v| serde_json::from_value(serde_json::Value::String(v)))
                    .transpose()?,
                terminated_at: row.terminated_at,
                termination_evidence: row
                    .termination_evidence
                    .map(|v| serde_json::from_value(serde_json::to_value(v)?))
                    .transpose()?,
                task_projected_at: row.task_projected_at,
                rejection: row
                    .rejection
                    .map(|v| serde_json::from_value(serde_json::Value::String(v)))
                    .transpose()?,
                result: row
                    .result
                    .map(|v| serde_json::from_value(serde_json::to_value(v)?))
                    .transpose()?,
            })
        };
        let file = decode().map_err(|_| ComputerError::Unavailable)?;
        file.authority
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        if row.id != super::record(row.transfer_id)
            || row.transfer_id.get_version_num() != 7
            || file.transfer_id() != row.transfer_id
            || file.computer_id() != row.computer_id
            || file.binding.provider_instance_id != row.provider_instance_id
            || row.actor_key != super::actor_key(&file.authority)?
            || file.binding.actor_key != row.actor_key
            || file.binding.owner_key != row.owner_key
            || row.task != file.task_id().record_id()
        {
            return Err(ComputerError::Unavailable);
        }
        file.binding
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        match file.dispatch_id {
            None => {
                if file.dispatched_at.is_some()
                    || file.execution_deadline.is_some()
                    || file.effective_limits.is_some()
                    || file.dispatch_authority.is_some()
                {
                    return Err(ComputerError::Unavailable);
                }
            }
            Some(_) => {
                if file.access.is_none() {
                    return Err(ComputerError::Unavailable);
                }
                let dispatched = file.dispatched_at.ok_or(ComputerError::Unavailable)?;
                let deadline = file.execution_deadline.ok_or(ComputerError::Unavailable)?;
                let limits = file.effective_limits.ok_or(ComputerError::Unavailable)?;
                if file.dispatch_id.is_none_or(|id| id.get_version_num() != 7)
                    || deadline <= dispatched
                    || dispatched < file.created_at
                    || !(1..=300).contains(&limits.maximum_seconds)
                    || !(1..=67108864).contains(&limits.maximum_bytes)
                    || deadline - dispatched
                        > chrono::TimeDelta::seconds(i64::from(limits.maximum_seconds))
                {
                    return Err(ComputerError::Unavailable);
                }
                file.dispatch_authority
                    .as_ref()
                    .ok_or(ComputerError::Unavailable)?
                    .validate(&file)?;
            }
        }
        file.validate_lifecycle()?;
        Ok(file)
    }
}

impl FileOperation {
    fn validate_lifecycle(&self) -> Result<()> {
        let failed = || ComputerError::Unavailable;
        if let Some(result) = &self.result {
            super::completion::validate_result(result, self).map_err(|_| failed())?;
            let settled = self.settled_at.ok_or_else(failed)?;
            if self.stage != FileTransferStage::Completed
                || self.dispatch_id.is_none()
                || self.containment_id.is_some()
                || self.refusal.is_some()
                || self.terminated_at.is_some()
                || self.termination_evidence.is_some()
                || settled < self.dispatched_at.ok_or_else(failed)?
                || settled
                    > self.execution_deadline.ok_or_else(failed)?
                        + chrono::TimeDelta::seconds(i64::from(super::FILE_PUBLICATION_SECONDS))
            {
                return Err(failed());
            }
        } else if self.stage == FileTransferStage::Completed {
            return Err(failed());
        }
        if let Some(reason) = self.rejection
            && (reason == veoveo_computer_execution::FileFailure::CommitUnknown
                || self.stage != FileTransferStage::Failed
                || self.result.is_some()
                || self.dispatch_id.is_none()
                || self.containment_id.is_some()
                || self.refusal.is_some()
                || self.terminated_at.is_some()
                || self.settled_at.is_none_or(|at| {
                    at < self.dispatched_at.unwrap_or(self.created_at)
                        || at
                            > self.execution_deadline.unwrap_or(self.created_at)
                                + chrono::TimeDelta::seconds(i64::from(
                                    super::FILE_PUBLICATION_SECONDS,
                                ))
                }))
        {
            return Err(failed());
        }
        if self.termination_evidence.is_some() != self.terminated_at.is_some() {
            return Err(failed());
        }
        if let Some(evidence) = self.termination_evidence {
            let expected = match evidence.kind {
                super::outcome::TerminationSource::Stop => self.containment_dispatch_id,
                super::outcome::TerminationSource::Read => self.last_containment_read_id,
            };
            if Some(evidence.id) != expected {
                return Err(failed());
            }
        }
        if let Some(id) = self.containment_id {
            let started = self.containment_started_at.ok_or_else(failed)?;
            let deadline = self.containment_deadline.ok_or_else(failed)?;
            if id.get_version_num() != 7
                || self.dispatch_id.is_none()
                || self.interruption.is_none()
                || started < self.dispatched_at.ok_or_else(failed)?
                || deadline - started != chrono::TimeDelta::seconds(180)
                || self.containment_reads > 8
                || self.last_containment_read_id.is_some() != (self.containment_reads > 0)
                || self.next_containment_read.is_some() != (self.containment_reads > 0)
                || self
                    .containment_dispatch_id
                    .is_some_and(|id| id.get_version_num() != 7)
                || self
                    .last_containment_read_id
                    .is_some_and(|id| id.get_version_num() != 7)
            {
                return Err(failed());
            }
        } else if self.interruption.is_some()
            || self.containment_dispatch_id.is_some()
            || self.containment_started_at.is_some()
            || self.containment_deadline.is_some()
            || self.containment_reads != 0
            || self.next_containment_read.is_some()
            || self.last_containment_read_id.is_some()
            || self.terminated_at.is_some()
        {
            return Err(failed());
        }
        match self.stage {
            FileTransferStage::Queued if self.dispatch_id.is_some() => return Err(failed()),
            FileTransferStage::Dispatched
                if self.dispatch_id.is_none() || self.containment_id.is_some() =>
            {
                return Err(failed());
            }
            FileTransferStage::Containing | FileTransferStage::RecoveryRequired
                if self.containment_id.is_none() =>
            {
                return Err(failed());
            }
            _ => {}
        }
        if self.is_terminal()
            && self.stage != FileTransferStage::Completed
            && self.rejection.is_none()
        {
            if self.settled_at.is_none_or(|at| at < self.created_at)
                || self.refusal.is_some() != self.dispatch_id.is_none()
                || self.terminated_at.is_some() != self.containment_id.is_some()
                || self
                    .terminated_at
                    .is_some_and(|at| Some(at) != self.settled_at)
                || (self.refusal.is_none() && self.containment_id.is_none())
                || (self.stage == FileTransferStage::Cancelled)
                    != (self.refusal == Some(super::FileRefusal::CancelledBeforeDispatch)
                        || self.interruption == Some(super::FileInterruption::Cancelled))
            {
                return Err(failed());
            }
        } else if self.stage != FileTransferStage::Completed
            && self.rejection.is_none()
            && (self.settled_at.is_some()
                || self.refusal.is_some()
                || self.terminated_at.is_some()
                || self.task_projected_at.is_some())
        {
            return Err(failed());
        }
        Ok(())
    }
}
