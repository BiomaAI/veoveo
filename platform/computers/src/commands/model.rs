use crate::{
    AcceptedAuthority, ComputerError, Result,
    command_secrets::{CommandBinding, SealedCommand},
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
}

/// Private durable command; public projections must select metadata explicitly.
pub struct CommandOperation {
    pub(super) binding: CommandBinding,
    pub(super) authority: AcceptedAuthority,
    pub(super) sealed: SealedCommand,
    pub(super) created_at: DateTime<Utc>,
    pub(super) stage: CommandStage,
    pub(super) dispatch_id: Option<Uuid>,
    pub(super) dispatched_at: Option<DateTime<Utc>>,
    pub(super) execution_deadline: Option<DateTime<Utc>>,
    pub(super) effective_limits: Option<crate::api::AutomationExecutionLimits>,
    pub(super) dispatch_authority: Option<super::CommandDispatchDecision>,
}
impl CommandOperation {
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
    task: RecordId,
    stage: String,
    created_at: DateTime<Utc>,
    dispatch_id: Option<Uuid>,
    dispatched_at: Option<DateTime<Utc>>,
    execution_deadline: Option<DateTime<Utc>>,
    effective_limits: Option<OpenObject>,
    dispatch_authority: Option<OpenObject>,
}
impl TryFrom<Record> for CommandOperation {
    type Error = ComputerError;
    fn try_from(row: Record) -> Result<Self> {
        let decode = || -> std::result::Result<_, serde_json::Error> {
            Ok(Self {
                binding: serde_json::from_value(serde_json::to_value(row.binding)?)?,
                authority: serde_json::from_value(serde_json::to_value(row.authority)?)?,
                sealed: serde_json::from_value(serde_json::to_value(row.sealed)?)?,
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
        match command.stage {
            CommandStage::Queued => {
                if command.dispatch_id.is_some()
                    || command.dispatched_at.is_some()
                    || command.execution_deadline.is_some()
                    || command.effective_limits.is_some()
                    || command.dispatch_authority.is_some()
                {
                    return Err(ComputerError::Unavailable);
                }
            }
            CommandStage::Dispatched => {
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
        Ok(command)
    }
}
