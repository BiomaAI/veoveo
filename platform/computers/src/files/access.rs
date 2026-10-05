//! Current public Task authority. Read metadata without loading protected payloads.
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result,
    api::{FileTransferDirection, FileTransferStage},
    task_access::TaskSelection,
};
use serde::Deserialize;
use std::time::{Duration, Instant};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::TaskOwner;

#[derive(Clone, Copy)]
pub enum FileTaskAction {
    Observe,
    Cancel,
}

/// The stored Task owner is a lookup boundary, never a replacement invocation
/// actor. Facades keep the authenticated caller for request audit and recheck this
/// deadline before releasing a response after a possibly slow Task read.
pub struct FileTaskAccess {
    owner: TaskOwner,
    computer_id: veoveo_computers_contract::ComputerId,
    transfer_id: veoveo_computers_contract::FileTransferId,
    deadline: Instant,
    direction: FileTransferDirection,
    stage: FileTransferStage,
    can_cancel: bool,
}
impl FileTaskAccess {
    pub fn owner(&self) -> Result<&TaskOwner> {
        if Instant::now() >= self.deadline {
            return Err(ComputerError::Forbidden);
        }
        Ok(&self.owner)
    }
    pub fn valid_until(&self) -> Instant {
        self.deadline
    }
    pub fn computer_id(&self) -> veoveo_computers_contract::ComputerId {
        self.computer_id
    }
    pub fn transfer_id(&self) -> veoveo_computers_contract::FileTransferId {
        self.transfer_id
    }
    pub fn direction(&self) -> FileTransferDirection {
        self.direction
    }
    pub fn stage(&self) -> FileTransferStage {
        self.stage
    }
    pub fn can_cancel(&self) -> bool {
        self.can_cancel
    }
}

#[derive(Deserialize, SurrealValue)]
struct Metadata {
    id: RecordId,
    transfer_id: Uuid,
    computer_id: Uuid,
    provider_instance_id: Uuid,
    actor_key: String,
    binding: crate::secrets::FileTransferBinding,
    authority: crate::AcceptedAuthority,
    task: RecordId,
    stage: String,
}

impl ComputersStore {
    /// Execute includes observing/cancelling this principal/client's own file
    /// Task. It does not grant collection, Computer, other Task or Artifact reads.
    /// The direct Computer owner can observe under current Read policy and cancel
    /// under current Stop policy, including after the agent grant was revoked.
    pub async fn authorize_file_task(
        &self,
        actor: &ComputerActor,
        transfer: veoveo_computers_contract::FileTransferId,
        action: FileTaskAction,
    ) -> Result<FileTaskAccess> {
        actor.check_admission()?;
        tokio::time::timeout(
            Duration::from_secs(5),
            self.file_access(actor, transfer, action),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    async fn file_access(
        &self,
        actor: &ComputerActor,
        transfer: veoveo_computers_contract::FileTransferId,
        action: FileTaskAction,
    ) -> Result<FileTaskAccess> {
        let permit = self
            .admit_task_metadata(
                actor,
                TaskSelection::File(transfer),
                super::actor_key(actor.accepted())?,
                matches!(action, FileTaskAction::Cancel),
            )
            .await?;
        let mut read = self
            .query(
                include_str!("../../queries/task_metadata.surql"),
                permit.bindings(self.provider_instance_id)?,
            )
            .await?;
        let row: Option<Metadata> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let row = row.ok_or(ComputerError::NotFound)?;
        let (binding, accepted) = (row.binding, row.authority);
        binding.validate().map_err(|_| ComputerError::Unavailable)?;
        accepted
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        if row.id != super::record(transfer)
            || row.transfer_id != transfer.as_uuid()
            || binding.transfer_id != transfer
            || row.computer_id != binding.computer_id.as_uuid()
            || row.provider_instance_id != binding.provider_instance_id.as_uuid()
            || row.actor_key != super::actor_key(&accepted)?
            || binding.actor_key != row.actor_key
            || row.task != task_record_id(transfer.task_id())
        {
            return Err(ComputerError::Unavailable);
        }
        if binding.provider_instance_id != self.provider_instance_id
            || accepted.invocation.tenant != actor.accepted().invocation.tenant
            || !binding.required_labels.iter().all(|label| {
                actor.owner().data_labels.contains(label.as_str())
                    && actor
                        .accepted()
                        .request_context
                        .principal
                        .data_labels
                        .contains(label)
            })
        {
            return Err(ComputerError::NotFound);
        }
        actor.check_admission()?;
        let access = FileTaskAccess {
            owner: accepted.task_owner(),
            computer_id: binding.computer_id,
            transfer_id: transfer,
            deadline: permit.valid_until(),
            direction: binding.direction,
            stage: serde_json::from_value(row.stage.into())
                .map_err(|_| ComputerError::Unavailable)?,
            can_cancel: permit.can_cancel(),
        };
        access.owner()?;
        Ok(access)
    }
}
