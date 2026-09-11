//! Current public Task authority. Read metadata without loading protected payloads.
use crate::{
    AcceptedAuthority, ComputerActor, ComputerError, ComputersStore, Result,
    api::{Action, AutomationPermission, FileTransferDirection, FileTransferStage},
    identity::owner_key,
    secrets::FileTransferBinding,
};
use serde::Deserialize;
use std::time::{Duration, Instant};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::{TaskId, TaskOwner};

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
    computer_id: Uuid,
    transfer_id: Uuid,
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
    pub fn computer_id(&self) -> Uuid {
        self.computer_id
    }
    pub fn transfer_id(&self) -> Uuid {
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
    binding: OpenObject,
    authority: OpenObject,
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
        transfer: Uuid,
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
        transfer: Uuid,
        action: FileTaskAction,
    ) -> Result<FileTaskAccess> {
        if transfer.get_version_num() != 7 {
            return Err(ComputerError::NotFound);
        }
        let mut read = self.query(
            "SELECT id, transfer_id, computer_id, provider_instance_id, actor_key, binding, authority, task, stage FROM ONLY $execution;",
            vec![("execution", super::record(transfer).into_value())],
        ).await?;
        let row: Option<Metadata> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let row = row.ok_or(ComputerError::NotFound)?;
        let decode =
            || -> std::result::Result<(FileTransferBinding, AcceptedAuthority), serde_json::Error> {
                Ok((
                    serde_json::from_value(serde_json::to_value(row.binding)?)?,
                    serde_json::from_value(serde_json::to_value(row.authority)?)?,
                ))
            };
        let (binding, accepted) = decode().map_err(|_| ComputerError::Unavailable)?;
        binding.validate().map_err(|_| ComputerError::Unavailable)?;
        accepted
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        if row.id != super::record(transfer)
            || row.transfer_id != transfer
            || binding.transfer_id != transfer
            || row.computer_id != binding.computer_id
            || row.provider_instance_id != binding.provider_instance_id
            || row.actor_key != super::actor_key(&accepted)?
            || binding.actor_key != row.actor_key
            || row.task != TaskId::from_uuid(transfer).record_id()
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
        let direct_owner = binding.owner_key == owner_key(actor.owner())?
            && actor.accepted().actor.id == actor.accepted().request_context.principal.id;
        let (deadline, can_cancel) = if direct_owner {
            let control = self.control_authority(actor).await?;
            control.require_read(Some(binding.computer_id))?;
            let computer = self.get(actor.owner(), binding.computer_id).await?;
            if computer.provider_instance_id != self.provider_instance_id {
                return Err(ComputerError::NotFound);
            }
            if matches!(action, FileTaskAction::Cancel) {
                control.require_action(Action::Stop)?;
            }
            (control.valid_until(), control.allows_action(Action::Stop))
        } else {
            if super::actor_key(actor.accepted())? != binding.actor_key {
                return Err(ComputerError::NotFound);
            }
            let authority = self
                .authorize_automation_grant(
                    actor,
                    binding.computer_id,
                    binding.grant_id.ok_or(ComputerError::NotFound)?,
                    AutomationPermission::Execute,
                )
                .await?;
            authority.require_file_transfer()?;
            if owner_key(&authority.computer()?.owner)? != binding.owner_key {
                return Err(ComputerError::NotFound);
            }
            (authority.valid_until(), true)
        };
        actor.check_admission()?;
        let access = FileTaskAccess {
            owner: accepted.task_owner(),
            computer_id: binding.computer_id,
            transfer_id: transfer,
            deadline,
            direction: binding.direction,
            stage: serde_json::from_value(row.stage.into())
                .map_err(|_| ComputerError::Unavailable)?,
            can_cancel,
        };
        access.owner()?;
        Ok(access)
    }
}
