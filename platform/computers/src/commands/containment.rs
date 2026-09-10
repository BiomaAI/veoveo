use super::{CommandInterruption, CommandOperation, CommandStage};
use crate::{ComputerError, ComputersStore, Result};
use chrono::{DateTime, Utc};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

/// One committed Stop submission. Losing it permits observation, never redispatch.
pub struct CommandContainmentStop {
    pub(super) operation: Box<CommandOperation>,
    pub(super) id: Uuid,
    deadline: Instant,
}
impl CommandContainmentStop {
    pub fn operation(&self) -> &CommandOperation {
        &self.operation
    }
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}
pub struct CommandContainmentRead {
    pub(super) operation: Box<CommandOperation>,
    pub(super) id: Uuid,
    deadline: Instant,
}
impl CommandContainmentRead {
    pub fn operation(&self) -> &CommandOperation {
        &self.operation
    }
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}
pub enum ContainmentReadAdmission {
    Read(CommandContainmentRead),
    Wait { until: DateTime<Utc> },
    RecoveryRequired,
}

impl ComputersStore {
    /// Uses the recorded stop_computer scope to contain only the admitted run.
    /// Revocation cannot withdraw this safety action from an already dispatched command.
    pub async fn begin_command_containment(
        &self,
        claim: &ClaimedTask,
        reason: CommandInterruption,
    ) -> Result<CommandOperation> {
        let mut operation = self.worker_command(claim).await?.operation;
        if operation.stage == CommandStage::Dispatched {
            operation.interruption = Some(reason);
        }
        self.commit_command(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/begin_command_containment.surql"),
            vec![
                ("containment_id", Uuid::now_v7().into_value()),
                (
                    "interruption",
                    serde_json::to_value(reason)
                        .map_err(|_| ComputerError::Unavailable)?
                        .as_str()
                        .ok_or(ComputerError::Unavailable)?
                        .to_owned()
                        .into_value(),
                ),
            ],
            "computer.execution_containment_requested",
        )
        .await?;
        self.command_for_claim(claim).await
    }

    /// Another lifecycle Stop may already own termination. In that case observe
    /// it; if it is definitively cancelled before dispatch, this original intent
    /// can acquire its first Stop ticket once the Computer returns to Ready.
    pub async fn admit_command_stop(
        &self,
        claim: &ClaimedTask,
    ) -> Result<Option<CommandContainmentStop>> {
        let operation = self.worker_command(claim).await?.operation;
        if operation.stage != CommandStage::Containing {
            return Err(ComputerError::StateConflict);
        }
        let id = Uuid::now_v7();
        self.commit_command(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/dispatch_command_stop.surql"),
            vec![("stop_id", id.into_value())],
            "computer.execution_stop_dispatched",
        )
        .await?;
        let selected = self.worker_command(claim).await?;
        if selected.operation.containment_dispatch_id != Some(id) {
            return Ok(None);
        }
        let remaining = selected
            .remaining(selected.operation.containment_deadline)
            .min(selected.remaining(Some(claim.lease_expires_at)))
            .min(Duration::from_secs(30));
        if selected.operation.stage != CommandStage::Containing || remaining.is_zero() {
            return Err(ComputerError::StateConflict);
        }
        Ok(Some(CommandContainmentStop {
            operation: Box::new(selected.operation),
            id,
            deadline: Instant::now() + remaining,
        }))
    }

    /// Charges the persistent budget before obtaining an authoritative Stop read.
    /// A new worker inherits the same count, backoff and absolute deadline.
    pub async fn admit_command_containment_read(
        &self,
        claim: &ClaimedTask,
    ) -> Result<ContainmentReadAdmission> {
        let operation = self.worker_command(claim).await?.operation;
        let id = Uuid::now_v7();
        let jitter = u64::from(operation.execution_id().as_bytes()[15]);
        let milliseconds = (500_u64 << operation.containment_reads.min(5)).min(10_000) + jitter;
        self.commit_command(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/observe_command_containment.surql"),
            vec![
                ("read_id", id.into_value()),
                (
                    "backoff",
                    surrealdb::types::Duration::from_std(Duration::from_millis(milliseconds))
                        .into_value(),
                ),
            ],
            "computer.execution_recovery_required",
        )
        .await?;
        let selected = self.worker_command(claim).await?;
        if selected.operation.stage == CommandStage::RecoveryRequired {
            return Ok(ContainmentReadAdmission::RecoveryRequired);
        }
        if selected.operation.last_containment_read_id == Some(id) {
            let remaining = selected
                .remaining(selected.operation.containment_deadline)
                .min(selected.remaining(Some(claim.lease_expires_at)))
                .min(Duration::from_secs(10));
            if remaining.is_zero() {
                return Err(ComputerError::StateConflict);
            }
            return Ok(ContainmentReadAdmission::Read(CommandContainmentRead {
                operation: Box::new(selected.operation),
                id,
                deadline: Instant::now() + remaining,
            }));
        }
        Ok(ContainmentReadAdmission::Wait {
            until: selected
                .operation
                .next_containment_read
                .ok_or(ComputerError::StateConflict)?,
        })
    }
}
