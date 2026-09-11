use super::{FileInterruption, FileOperation, FileTransferStage};
use crate::{ComputerError, ComputersStore, Result};
use chrono::{DateTime, Utc};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

/// One committed Stop submission. Losing it permits observation, never redispatch.
pub struct FileContainmentStop {
    pub(super) operation: Box<FileOperation>,
    pub(super) id: Uuid,
    deadline: Instant,
}
impl FileContainmentStop {
    pub fn operation(&self) -> &FileOperation {
        &self.operation
    }
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}
pub struct FileContainmentRead {
    pub(super) operation: Box<FileOperation>,
    pub(super) id: Uuid,
    deadline: Instant,
}
impl FileContainmentRead {
    pub fn operation(&self) -> &FileOperation {
        &self.operation
    }
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}
pub enum ContainmentReadAdmission {
    Read(FileContainmentRead),
    Wait { until: DateTime<Utc> },
    RecoveryRequired,
}

impl ComputersStore {
    /// Uses the recorded stop_computer scope to contain only the admitted run.
    /// Revocation cannot withdraw this safety action from an already dispatched file.
    pub async fn begin_file_containment(
        &self,
        claim: &ClaimedTask,
        reason: FileInterruption,
    ) -> Result<FileOperation> {
        let mut operation = self.worker_file(claim).await?.operation;
        if operation.stage == FileTransferStage::Dispatched {
            operation.interruption = Some(reason);
        }
        self.commit_file(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/begin_file_containment.surql"),
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
            "computer.file_transfer_containment_requested",
        )
        .await?;
        self.file_for_claim(claim).await
    }

    /// Another lifecycle Stop may already own termination. In that case observe
    /// it; if it is definitively cancelled before dispatch, this original intent
    /// can acquire its first Stop ticket once the Computer returns to Ready.
    pub async fn admit_file_stop(
        &self,
        claim: &ClaimedTask,
    ) -> Result<Option<FileContainmentStop>> {
        let operation = self.worker_file(claim).await?.operation;
        if operation.stage != FileTransferStage::Containing {
            return Err(ComputerError::StateConflict);
        }
        let id = Uuid::now_v7();
        self.commit_file(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/dispatch_file_stop.surql"),
            vec![("stop_id", id.into_value())],
            "computer.file_transfer_stop_dispatched",
        )
        .await?;
        let selected = self.worker_file(claim).await?;
        if selected.operation.containment_dispatch_id != Some(id) {
            return Ok(None);
        }
        let remaining = selected
            .remaining(selected.operation.containment_deadline)
            .min(selected.remaining(Some(claim.lease_expires_at)))
            .min(Duration::from_secs(30));
        if selected.operation.stage != FileTransferStage::Containing || remaining.is_zero() {
            return Err(ComputerError::StateConflict);
        }
        Ok(Some(FileContainmentStop {
            operation: Box::new(selected.operation),
            id,
            deadline: Instant::now() + remaining,
        }))
    }

    /// Charges the persistent budget before obtaining an authoritative Stop read.
    /// A new worker inherits the same count, backoff and absolute deadline.
    pub async fn admit_file_containment_read(
        &self,
        claim: &ClaimedTask,
    ) -> Result<ContainmentReadAdmission> {
        let operation = self.worker_file(claim).await?.operation;
        let id = Uuid::now_v7();
        let jitter = u64::from(operation.transfer_id().as_bytes()[15]);
        let milliseconds = (500_u64 << operation.containment_reads.min(5)).min(10_000) + jitter;
        self.commit_file(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/observe_file_containment.surql"),
            vec![
                ("read_id", id.into_value()),
                (
                    "backoff",
                    surrealdb::types::Duration::from_std(Duration::from_millis(milliseconds))
                        .into_value(),
                ),
            ],
            "computer.file_transfer_recovery_required",
        )
        .await?;
        let selected = self.worker_file(claim).await?;
        if selected.operation.stage == FileTransferStage::RecoveryRequired {
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
            return Ok(ContainmentReadAdmission::Read(FileContainmentRead {
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
