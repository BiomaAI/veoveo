//! Current public Task authority. Read metadata without loading protected payloads.
use crate::{
    AcceptedAuthority, ComputerActor, ComputerError, ComputersStore, Result,
    secrets::CommandBinding, task_access::TaskSelection,
};
use serde::Deserialize;
use std::time::{Duration, Instant};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::TaskOwner;

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
    computer_id: veoveo_computers_contract::ComputerId,
    execution_id: veoveo_computers_contract::ExecutionId,
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
    pub fn computer_id(&self) -> veoveo_computers_contract::ComputerId {
        self.computer_id
    }
    pub fn execution_id(&self) -> veoveo_computers_contract::ExecutionId {
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
        execution: veoveo_computers_contract::ExecutionId,
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
        execution: veoveo_computers_contract::ExecutionId,
        action: CommandTaskAction,
    ) -> Result<CommandTaskAccess> {
        let permit = self
            .admit_task_metadata(
                actor,
                TaskSelection::Command(execution),
                super::actor_key(actor.accepted())?,
                matches!(action, CommandTaskAction::Cancel),
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
            || row.execution_id != execution.into_uuid()
            || binding.execution_id != execution
            || row.computer_id != binding.computer_id.into_uuid()
            || row.provider_instance_id != binding.provider_instance_id
            || row.actor_key != super::actor_key(&accepted)?
            || binding.actor_key != row.actor_key
            || row.task != task_record_id(execution.task_id())
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
        actor.check_admission()?;
        let access = CommandTaskAccess {
            owner: accepted.task_owner(),
            computer_id: binding.computer_id,
            execution_id: execution,
            deadline: permit.valid_until(),
        };
        access.owner()?;
        Ok(access)
    }
}
