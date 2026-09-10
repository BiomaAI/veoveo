use crate::{
    AcceptedAuthority, ComputerError, Result,
    command_secrets::{CommandBinding, SealedCommand, SealedOutputAccess},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::{TaskId, TaskOwner};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStage {
    Queued,
    Dispatched,
    Containing,
    RecoveryRequired,
    Failed,
    Cancelled,
}

/// Private durable command; public projections must select metadata explicitly.
pub struct CommandOperation {
    pub(super) binding: CommandBinding,
    pub(super) authority: AcceptedAuthority,
    pub(super) sealed: SealedCommand,
    pub(super) output_access: Option<SealedOutputAccess>,
    pub(super) created_at: DateTime<Utc>,
    pub(super) stage: CommandStage,
    pub(super) dispatch_id: Option<Uuid>,
    pub(super) dispatched_at: Option<DateTime<Utc>>,
    pub(super) execution_deadline: Option<DateTime<Utc>>,
    pub(super) effective_limits: Option<crate::api::AutomationExecutionLimits>,
    pub(super) dispatch_authority: Option<super::CommandDispatchDecision>,
    pub(super) containment_id: Option<Uuid>,
    pub(super) interruption: Option<super::CommandInterruption>,
    pub(super) containment_dispatch_id: Option<Uuid>,
    pub(super) containment_started_at: Option<DateTime<Utc>>,
    pub(super) containment_deadline: Option<DateTime<Utc>>,
    pub(super) containment_reads: u32,
    pub(super) next_containment_read: Option<DateTime<Utc>>,
    pub(super) last_containment_read_id: Option<Uuid>,
    pub(super) settled_at: Option<DateTime<Utc>>,
    pub(super) refusal: Option<super::CommandRefusal>,
    pub(super) terminated_at: Option<DateTime<Utc>>,
    pub(super) termination_evidence: Option<super::outcome::TerminationEvidence>,
    pub(super) task_projected_at: Option<DateTime<Utc>>,
}
impl CommandOperation {
    pub fn binding(&self) -> &CommandBinding {
        &self.binding
    }
    pub fn is_terminal(&self) -> bool {
        matches!(self.stage, CommandStage::Failed | CommandStage::Cancelled)
    }
    pub fn outcome(&self) -> Option<super::CommandOutcome> {
        if !self.is_terminal() {
            return None;
        }
        self.refusal
            .map(super::CommandOutcome::Undispatched)
            .or_else(|| self.interruption.map(super::CommandOutcome::Terminated))
    }
    pub fn containment_id(&self) -> Option<Uuid> {
        self.containment_id
    }
    pub fn task_projected(&self) -> bool {
        self.task_projected_at.is_some()
    }
    pub fn execution_id(&self) -> Uuid {
        self.binding.execution_id
    }
    pub fn stage(&self) -> CommandStage {
        self.stage
    }
    pub fn execution_deadline(&self) -> Option<DateTime<Utc>> {
        self.execution_deadline
    }
    pub fn computer_id(&self) -> Uuid {
        self.binding.computer_id
    }
    pub fn task_id(&self) -> TaskId {
        TaskId::from_uuid(self.execution_id())
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
            execution_id: Uuid,
        }
        serde_json::to_value(Reference {
            computer_id: self.computer_id(),
            execution_id: self.execution_id(),
        })
        .map_err(|_| ComputerError::Unavailable)
    }
}

#[derive(Deserialize, SurrealValue)]
pub(super) struct Record {
    id: RecordId,
    execution_id: Uuid,
    computer_id: Uuid,
    provider_instance_id: Uuid,
    actor_key: String,
    binding: OpenObject,
    authority: OpenObject,
    sealed: OpenObject,
    output_access: Option<OpenObject>,
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
}
impl TryFrom<Record> for CommandOperation {
    type Error = ComputerError;
    fn try_from(row: Record) -> Result<Self> {
        let decode = || -> std::result::Result<_, serde_json::Error> {
            Ok(Self {
                binding: serde_json::from_value(serde_json::to_value(row.binding)?)?,
                authority: serde_json::from_value(serde_json::to_value(row.authority)?)?,
                sealed: serde_json::from_value(serde_json::to_value(row.sealed)?)?,
                output_access: row
                    .output_access
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
            })
        };
        let command = decode().map_err(|_| ComputerError::Unavailable)?;
        command
            .authority
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        if row.id != super::record(row.execution_id)
            || row.execution_id.get_version_num() != 7
            || command.execution_id() != row.execution_id
            || command.computer_id() != row.computer_id
            || command.binding.provider_instance_id != row.provider_instance_id
            || row.actor_key != super::actor_key(&command.authority)?
            || command.binding.actor_key != row.actor_key
            || row.task != command.task_id().record_id()
        {
            return Err(ComputerError::Unavailable);
        }
        match command.dispatch_id {
            None => {
                if command.dispatch_id.is_some()
                    || command.dispatched_at.is_some()
                    || command.execution_deadline.is_some()
                    || command.effective_limits.is_some()
                    || command.dispatch_authority.is_some()
                {
                    return Err(ComputerError::Unavailable);
                }
            }
            Some(_) => {
                let dispatched = command.dispatched_at.ok_or(ComputerError::Unavailable)?;
                let deadline = command
                    .execution_deadline
                    .ok_or(ComputerError::Unavailable)?;
                let limits = command.effective_limits.ok_or(ComputerError::Unavailable)?;
                if command
                    .dispatch_id
                    .is_none_or(|id| id.get_version_num() != 7)
                    || deadline <= dispatched
                    || dispatched < command.created_at
                    || !(1..=7200).contains(&limits.maximum_seconds)
                    || !(1..=67108864).contains(&limits.maximum_output_bytes)
                    || deadline - dispatched
                        > chrono::TimeDelta::seconds(i64::from(limits.maximum_seconds))
                {
                    return Err(ComputerError::Unavailable);
                }
                command
                    .dispatch_authority
                    .as_ref()
                    .ok_or(ComputerError::Unavailable)?
                    .validate(&command)?;
            }
        }
        command.validate_lifecycle()?;
        Ok(command)
    }
}

impl CommandOperation {
    fn validate_lifecycle(&self) -> Result<()> {
        let failed = || ComputerError::Unavailable;
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
            CommandStage::Queued if self.dispatch_id.is_some() => return Err(failed()),
            CommandStage::Dispatched
                if self.dispatch_id.is_none() || self.containment_id.is_some() =>
            {
                return Err(failed());
            }
            CommandStage::Containing | CommandStage::RecoveryRequired
                if self.containment_id.is_none() =>
            {
                return Err(failed());
            }
            _ => {}
        }
        if self.is_terminal() {
            if self.settled_at.is_none_or(|at| at < self.created_at)
                || self.refusal.is_some() != self.dispatch_id.is_none()
                || self.terminated_at.is_some() != self.containment_id.is_some()
                || self
                    .terminated_at
                    .is_some_and(|at| Some(at) != self.settled_at)
                || (self.refusal.is_none() && self.containment_id.is_none())
                || (self.stage == CommandStage::Cancelled)
                    != (self.refusal == Some(super::CommandRefusal::CancelledBeforeDispatch)
                        || self.interruption == Some(super::CommandInterruption::Cancelled))
            {
                return Err(failed());
            }
        } else if self.settled_at.is_some()
            || self.refusal.is_some()
            || self.terminated_at.is_some()
            || self.task_projected_at.is_some()
        {
            return Err(failed());
        }
        Ok(())
    }
}
