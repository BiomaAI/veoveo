use crate::{
    AcceptedAuthority, ComputerError, Result,
    secrets::{FileTransferBinding, SealedFileTransfer, SealedFileTransferAccess},
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::{TaskId, TaskOwner};

/// Private journal. Public facades select authorized metadata, never serialize it.
pub struct FileOperation {
    pub(super) binding: FileTransferBinding,
    pub(super) authority: AcceptedAuthority,
    pub(super) sealed: SealedFileTransfer,
    pub(super) access: Option<SealedFileTransferAccess>,
    pub(super) created_at: DateTime<Utc>,
}
impl FileOperation {
    pub fn transfer_id(&self) -> Uuid {
        self.binding.transfer_id
    }
    pub fn computer_id(&self) -> Uuid {
        self.binding.computer_id
    }
    pub fn task_id(&self) -> TaskId {
        TaskId::from_uuid(self.transfer_id())
    }
    pub fn binding(&self) -> &FileTransferBinding {
        &self.binding
    }
    pub fn actor(&self) -> TaskOwner {
        self.authority.task_owner()
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn stage(&self) -> crate::api::FileTransferStage {
        crate::api::FileTransferStage::Queued
    }
    pub(super) fn task_reference(&self) -> Result<serde_json::Value> {
        #[derive(serde::Serialize)]
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
    owner_key: String,
    actor_key: String,
    binding: OpenObject,
    authority: OpenObject,
    sealed: OpenObject,
    artifact_access: Option<OpenObject>,
    task: RecordId,
    stage: String,
    created_at: DateTime<Utc>,
}
impl TryFrom<Record> for FileOperation {
    type Error = ComputerError;
    fn try_from(row: Record) -> Result<Self> {
        let decode = || -> std::result::Result<Self, serde_json::Error> {
            Ok(Self {
                binding: serde_json::from_value(serde_json::to_value(row.binding)?)?,
                authority: serde_json::from_value(serde_json::to_value(row.authority)?)?,
                sealed: serde_json::from_value(serde_json::to_value(row.sealed)?)?,
                access: row
                    .artifact_access
                    .map(|value| serde_json::from_value(serde_json::to_value(value)?))
                    .transpose()?,
                created_at: row.created_at,
            })
        };
        let operation = decode().map_err(|_| ComputerError::Unavailable)?;
        operation
            .authority
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        operation
            .binding
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        if row.id != super::record(row.transfer_id)
            || row.transfer_id != operation.transfer_id()
            || row.computer_id != operation.computer_id()
            || row.provider_instance_id != operation.binding.provider_instance_id
            || row.actor_key != super::actor_key(&operation.authority)?
            || row.actor_key != operation.binding.actor_key
            || row.owner_key != operation.binding.owner_key
            || row.task != operation.task_id().record_id()
            || row.stage != "queued"
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(operation)
    }
}
