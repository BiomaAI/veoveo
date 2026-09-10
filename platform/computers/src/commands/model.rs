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

/// Private durable command; public projections must select metadata explicitly.
pub struct QueuedCommand {
    pub(super) binding: CommandBinding,
    pub(super) authority: AcceptedAuthority,
    pub(super) sealed: SealedCommand,
    pub(super) created_at: DateTime<Utc>,
}
impl QueuedCommand {
    pub fn execution_id(&self) -> Uuid {
        self.binding.execution_id
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
}
impl TryFrom<Record> for QueuedCommand {
    type Error = ComputerError;
    fn try_from(row: Record) -> Result<Self> {
        let decode = || -> std::result::Result<_, serde_json::Error> {
            Ok(Self {
                binding: serde_json::from_value(serde_json::to_value(row.binding)?)?,
                authority: serde_json::from_value(serde_json::to_value(row.authority)?)?,
                sealed: serde_json::from_value(serde_json::to_value(row.sealed)?)?,
                created_at: row.created_at,
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
            || row.stage != "queued"
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(command)
    }
}
