use super::{CommandInterruption, CommandOperation, CommandStage};
use crate::{ComputerError, ComputersStore, Result, api::AutomationPermission};
use chrono::{DateTime, TimeDelta, Utc};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::{ClaimedTask, TaskRuntime, TaskStatus};

pub(super) const AUTHORITY_WINDOW: Duration = Duration::from_secs(5);

/// Current authority for an already dispatched foreground stream. This never
/// creates a second dispatch ticket or extends the original runtime/output limits.
pub struct CommandRunAuthority {
    pub valid_until: Instant,
    pub execution_deadline: Instant,
    pub maximum_output_bytes: u32,
    pub deadline_reason: CommandInterruption,
}
pub enum CommandContinuation {
    Authorized(CommandRunAuthority),
    Interrupted(CommandInterruption),
}

impl ComputersStore {
    /// The caller enforces its previous deadline while this bounded read is in
    /// flight. A blocked refresh cannot extend current execution authority.
    pub async fn command_continuation(
        &self,
        claim: &ClaimedTask,
        operation: &CommandOperation,
    ) -> Result<CommandContinuation> {
        tokio::time::timeout(
            Duration::from_secs(4),
            self.read_command_continuation(claim, operation),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    async fn read_command_continuation(
        &self,
        claim: &ClaimedTask,
        operation: &CommandOperation,
    ) -> Result<CommandContinuation> {
        if operation.stage != CommandStage::Dispatched
            || operation.binding.provider_instance_id != self.provider_instance_id
            || operation.task_id() != claim.snapshot.task_id
            || operation.actor() != claim.snapshot.owner
            || operation.task_reference()? != claim.snapshot.request
        {
            return Err(ComputerError::InvalidState);
        }
        let object = |value: serde_json::Value| -> Result<OpenObject> {
            serde_json::from_value(value).map_err(|_| ComputerError::Unavailable)
        };
        let began = Instant::now();
        // Compare immutable metadata in the database. Never fetch or decrypt the
        // potentially two-megabyte command/capability envelope on every refresh.
        let mut response = self
            .query(
                include_str!("../../queries/command_continuation.surql"),
                vec![
                    (
                        "execution",
                        super::record(operation.execution_id()).into_value(),
                    ),
                    ("provider", self.provider_instance_id.into_value()),
                    ("task", operation.task_id().record_id().into_value()),
                    ("dispatch_id", operation.dispatch_id.into_value()),
                    (
                        "binding",
                        object(
                            serde_json::to_value(&operation.binding)
                                .map_err(|_| ComputerError::Unavailable)?,
                        )?
                        .into_value(),
                    ),
                    (
                        "authority",
                        object(
                            serde_json::to_value(&operation.authority)
                                .map_err(|_| ComputerError::Unavailable)?,
                        )?
                        .into_value(),
                    ),
                    (
                        "limits",
                        object(
                            serde_json::to_value(
                                operation
                                    .effective_limits
                                    .ok_or(ComputerError::Unavailable)?,
                            )
                            .map_err(|_| ComputerError::Unavailable)?,
                        )?
                        .into_value(),
                    ),
                    ("dispatched_at", operation.dispatched_at.into_value()),
                    (
                        "execution_deadline",
                        operation.execution_deadline.into_value(),
                    ),
                ],
            )
            .await?;
        let valid: Option<bool> = response.take(1).map_err(|_| ComputerError::Unavailable)?;
        let database_time: Option<DateTime<Utc>> =
            response.take(2).map_err(|_| ComputerError::Unavailable)?;
        if valid != Some(true) {
            return Err(ComputerError::StateConflict);
        }
        let database_time = database_time.ok_or(ComputerError::Unavailable)?;
        let remaining_until = |until: DateTime<Utc>| {
            (until - database_time)
                .to_std()
                .unwrap_or_default()
                .saturating_sub(began.elapsed())
        };
        let task = TaskRuntime::new(self.platform.clone(), "computers", &claim.lease_owner)
            .get(&operation.task_id().to_string())
            .await
            .map_err(|_| ComputerError::Unavailable)?
            .ok_or(ComputerError::StateConflict)?;
        if task.lease_owner.as_deref() != Some(&claim.lease_owner)
            || task.lease_expires_at != Some(claim.lease_expires_at)
            || task.is_terminal()
            || task.owner != operation.actor()
        {
            return Err(ComputerError::StateConflict);
        }
        if task.status == TaskStatus::CancelRequested {
            return Ok(CommandContinuation::Interrupted(
                CommandInterruption::Cancelled,
            ));
        }
        let permit = match self
            .authorize_accepted_automation(
                &operation.authority,
                operation.computer_id(),
                operation.binding.grant_id,
                AutomationPermission::Execute,
            )
            .await
        {
            Ok(permit) => permit,
            Err(ComputerError::Forbidden | ComputerError::NotFound) => {
                return Ok(CommandContinuation::Interrupted(
                    CommandInterruption::AuthorityLost,
                ));
            }
            Err(error) => return Err(error),
        };
        let current = permit.computer()?;
        let binding = &operation.binding;
        if current.provider_instance_id != binding.provider_instance_id
            || crate::identity::owner_key(&current.owner)? != binding.owner_key
            || current.template_fingerprint != binding.template_fingerprint
            || current.provider_resource_id.as_deref() != Some(&binding.resource_id)
            || current.process_id.as_deref() != Some(&binding.process_id)
        {
            return Ok(CommandContinuation::Interrupted(
                CommandInterruption::ExecutionUnknown,
            ));
        }
        let original = operation
            .effective_limits
            .ok_or(ComputerError::Unavailable)?;
        let limits = permit.execution_limits()?.ok_or(ComputerError::Forbidden)?;
        let deadline = (operation.dispatched_at.ok_or(ComputerError::Unavailable)?
            + TimeDelta::seconds(i64::from(
                original.maximum_seconds.min(limits.maximum_seconds),
            )))
        .min(
            operation
                .execution_deadline
                .ok_or(ComputerError::Unavailable)?,
        )
        .min(permit.expires_at());
        let reason = if deadline
            < operation
                .execution_deadline
                .ok_or(ComputerError::Unavailable)?
        {
            CommandInterruption::AuthorityLost
        } else {
            CommandInterruption::Deadline
        };
        let remaining = remaining_until(deadline);
        if remaining.is_zero() {
            return Ok(CommandContinuation::Interrupted(reason));
        }
        let valid_until = (began + AUTHORITY_WINDOW)
            .min(permit.valid_until())
            .min(Instant::now() + remaining_until(claim.lease_expires_at));
        if valid_until <= Instant::now() {
            return Err(ComputerError::Unavailable);
        }
        Ok(CommandContinuation::Authorized(CommandRunAuthority {
            valid_until,
            execution_deadline: Instant::now() + remaining,
            maximum_output_bytes: original
                .maximum_output_bytes
                .min(limits.maximum_output_bytes),
            deadline_reason: reason,
        }))
    }
}
