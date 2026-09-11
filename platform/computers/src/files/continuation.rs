use super::{FileInterruption, FileOperation, FileTransferStage};
use crate::{ComputerError, ComputersStore, Result};
use chrono::{DateTime, TimeDelta, Utc};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::{ClaimedTask, TaskRuntime, TaskStatus};

pub(super) const AUTHORITY_WINDOW: Duration = Duration::from_secs(5);

/// Current authority for preparation or an already dispatched foreground stream.
/// This never creates a dispatch ticket or extends the original transfer limits.
pub struct FileRunAuthority {
    pub valid_until: Instant,
    pub execution_deadline: Instant,
    pub maximum_bytes: u64,
    pub deadline_reason: FileInterruption,
}
pub enum FileContinuation {
    Authorized(FileRunAuthority),
    Interrupted(FileInterruption),
}

impl ComputersStore {
    /// The caller enforces its previous deadline while this bounded read is in
    /// flight. A blocked refresh cannot extend current execution authority.
    pub async fn file_continuation(
        &self,
        claim: &ClaimedTask,
        operation: &FileOperation,
    ) -> Result<FileContinuation> {
        tokio::time::timeout(
            Duration::from_secs(4),
            self.read_file_continuation(claim, operation),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    async fn read_file_continuation(
        &self,
        claim: &ClaimedTask,
        operation: &FileOperation,
    ) -> Result<FileContinuation> {
        if !matches!(
            operation.stage,
            FileTransferStage::Queued | FileTransferStage::Dispatched
        ) || operation.binding.provider_instance_id != self.provider_instance_id
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
        // encrypted file intent and capability on every refresh.
        let mut response = self
            .query(
                include_str!("../../queries/file_continuation.surql"),
                vec![
                    (
                        "execution",
                        super::record(operation.transfer_id()).into_value(),
                    ),
                    ("provider", self.provider_instance_id.into_value()),
                    ("task", operation.task_id().record_id().into_value()),
                    ("dispatch_id", operation.dispatch_id.into_value()),
                    (
                        "stage",
                        match operation.stage {
                            FileTransferStage::Queued => "queued",
                            _ => "dispatched",
                        }
                        .into_value(),
                    ),
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
                        operation
                            .effective_limits
                            .as_ref()
                            .map(super::object)
                            .transpose()?
                            .into_value(),
                    ),
                    ("dispatched_at", operation.dispatched_at.into_value()),
                    ("created_at", operation.created_at.into_value()),
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
            return Ok(FileContinuation::Interrupted(FileInterruption::Cancelled));
        }
        let permit = match self.accepted_file_authority(operation).await {
            Ok(permit) => permit,
            Err(ComputerError::Forbidden | ComputerError::NotFound) => {
                return Ok(FileContinuation::Interrupted(
                    FileInterruption::AuthorityLost,
                ));
            }
            Err(error) => return Err(error),
        };
        let current = permit.computer()?;
        let binding = &operation.binding;
        if current.provider_instance_id != binding.provider_instance_id
            || current
                .replacement_instance_id
                .unwrap_or(current.computer_id)
                != binding.instance_id
            || crate::identity::owner_key(&current.owner)? != binding.owner_key
            || current.template_fingerprint != binding.template_fingerprint
            || current.provider_resource_id.as_deref() != Some(&binding.resource_id)
            || current.process_id.as_deref() != Some(&binding.process_id)
        {
            return Ok(FileContinuation::Interrupted(
                FileInterruption::ExecutionUnknown,
            ));
        }
        let limits = permit.limits()?;
        let original = operation.effective_limits.unwrap_or(limits);
        let original_deadline = operation.execution_deadline.unwrap_or(
            operation.created_at + TimeDelta::seconds(i64::from(super::FILE_PREPARATION_SECONDS)),
        );
        let deadline = if operation.stage == FileTransferStage::Queued {
            original_deadline.min(
                permit
                    .execution_expiry()
                    .unwrap_or(chrono::DateTime::<Utc>::MAX_UTC),
            )
        } else {
            (operation.dispatched_at.ok_or(ComputerError::Unavailable)?
                + TimeDelta::seconds(i64::from(
                    original.maximum_seconds.min(limits.maximum_seconds),
                )))
            .min(
                operation
                    .execution_deadline
                    .ok_or(ComputerError::Unavailable)?,
            )
            .min(
                permit
                    .execution_expiry()
                    .unwrap_or(chrono::DateTime::<Utc>::MAX_UTC),
            )
        };
        let reason = if deadline < original_deadline {
            FileInterruption::AuthorityLost
        } else {
            FileInterruption::Deadline
        };
        let remaining = remaining_until(deadline);
        if remaining.is_zero() {
            return Ok(FileContinuation::Interrupted(reason));
        }
        let valid_until = (began + AUTHORITY_WINDOW)
            .min(permit.valid_until())
            .min(Instant::now() + remaining_until(claim.lease_expires_at));
        if valid_until <= Instant::now() {
            return Err(ComputerError::Unavailable);
        }
        Ok(FileContinuation::Authorized(FileRunAuthority {
            valid_until,
            execution_deadline: Instant::now() + remaining,
            maximum_bytes: original.maximum_bytes.min(limits.maximum_bytes),
            deadline_reason: reason,
        }))
    }
}
