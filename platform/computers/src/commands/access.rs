//! Current public Task authority. Read metadata without loading protected payloads.
use crate::{
    AcceptedAuthority, ComputerActor, ComputerError, ComputersStore, Result,
    api::{Action, AutomationPermission},
    command_secrets::CommandBinding,
    identity::owner_key,
};
use serde::Deserialize;
use std::time::{Duration, Instant};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::{TaskId, TaskOwner};

#[derive(Clone, Copy)]
pub enum CommandTaskAction {
    Observe,
    Cancel,
}

/// The stored Task owner is a lookup boundary, never a replacement invocation
/// actor. Facades keep the authenticated caller for request audit and recheck this
/// deadline before releasing a response after a possibly slow Task read.
pub struct CommandTaskAccess {
    owner: TaskOwner,
    computer_id: Uuid,
    execution_id: Uuid,
    deadline: Instant,
}
impl CommandTaskAccess {
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
    pub fn execution_id(&self) -> Uuid {
        self.execution_id
    }
}

#[derive(Deserialize, SurrealValue)]
struct Metadata {
    id: RecordId,
    execution_id: Uuid,
    computer_id: Uuid,
    provider_instance_id: Uuid,
    actor_key: String,
    binding: OpenObject,
    authority: OpenObject,
    task: RecordId,
}

impl ComputersStore {
    /// Execute includes observing/cancelling this principal/client's own command
    /// Task. It does not grant collection, Computer, other Task or Artifact reads.
    /// The direct Computer owner can observe under current Read policy and cancel
    /// under current Stop policy, including after the agent grant was revoked.
    pub async fn authorize_command_task(
        &self,
        actor: &ComputerActor,
        execution: Uuid,
        action: CommandTaskAction,
    ) -> Result<CommandTaskAccess> {
        actor.check_admission()?;
        tokio::time::timeout(
            Duration::from_secs(5),
            self.command_access(actor, execution, action),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    async fn command_access(
        &self,
        actor: &ComputerActor,
        execution: Uuid,
        action: CommandTaskAction,
    ) -> Result<CommandTaskAccess> {
        if execution.get_version_num() != 7 {
            return Err(ComputerError::NotFound);
        }
        let mut read = self.query(
            "SELECT id, execution_id, computer_id, provider_instance_id, actor_key, binding, authority, task FROM ONLY $execution;",
            vec![("execution", super::record(execution).into_value())],
        ).await?;
        let row: Option<Metadata> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let row = row.ok_or(ComputerError::NotFound)?;
        let decode =
            || -> std::result::Result<(CommandBinding, AcceptedAuthority), serde_json::Error> {
                Ok((
                    serde_json::from_value(serde_json::to_value(row.binding)?)?,
                    serde_json::from_value(serde_json::to_value(row.authority)?)?,
                ))
            };
        let (binding, accepted) = decode().map_err(|_| ComputerError::Unavailable)?;
        accepted
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        if row.id != super::record(execution)
            || row.execution_id != execution
            || binding.execution_id != execution
            || row.computer_id != binding.computer_id
            || row.provider_instance_id != binding.provider_instance_id
            || row.actor_key != super::actor_key(&accepted)?
            || binding.actor_key != row.actor_key
            || row.task != TaskId::from_uuid(execution).record_id()
        {
            return Err(ComputerError::Unavailable);
        }
        if binding.provider_instance_id != self.provider_instance_id
            || accepted.invocation.tenant != actor.accepted().invocation.tenant
            || !binding.required_output_labels.iter().all(|label| {
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
        let deadline = if direct_owner {
            let control = self.control_authority(actor).await?;
            control.require_read(Some(binding.computer_id))?;
            let computer = self.get(actor.owner(), binding.computer_id).await?;
            if computer.provider_instance_id != self.provider_instance_id {
                return Err(ComputerError::NotFound);
            }
            if matches!(action, CommandTaskAction::Cancel) {
                control.require_action(Action::Stop)?;
            }
            control.valid_until()
        } else {
            if super::actor_key(actor.accepted())? != binding.actor_key {
                return Err(ComputerError::NotFound);
            }
            let authority = self
                .authorize_automation_grant(
                    actor,
                    binding.computer_id,
                    binding.grant_id,
                    AutomationPermission::Execute,
                )
                .await?;
            if owner_key(&authority.computer()?.owner)? != binding.owner_key {
                return Err(ComputerError::NotFound);
            }
            authority.valid_until()
        };
        actor.check_admission()?;
        let access = CommandTaskAccess {
            owner: accepted.task_owner(),
            computer_id: binding.computer_id,
            execution_id: execution,
            deadline,
        };
        access.owner()?;
        Ok(access)
    }
}
