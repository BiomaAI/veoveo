use super::FileOperation;
use crate::{ComputerError, ComputersStore, Result};
use std::collections::BTreeSet;
use surrealdb::types::SurrealValue;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskRetentionPin, TaskRuntime};

impl ComputersStore {
    /// Trusted worker discovery; public projections use current principal authority.
    pub async fn pending_file_transfers(
        &self,
        after: Option<uuid::Uuid>,
        limit: u32,
    ) -> Result<Vec<FileOperation>> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        let mut response = self
            .query(
                "SELECT * FROM computer_file_transfer WHERE provider_instance_id = $provider
             AND ($after = NONE OR transfer_id > $after)
             AND (stage != 'recovery_required' OR task.status = NONE OR task.status IN ['queued', 'running'])
             AND (task_projected_at = NONE OR task.retention_pins CONTAINS string::concat('computer-file-transfer/', <string>transfer_id))
             ORDER BY transfer_id LIMIT $limit;",
                vec![
                    ("provider", self.provider_instance_id.into_value()),
                    ("after", after.into_value()),
                    ("limit", limit.into_value()),
                ],
            )
            .await?;
        let records: Vec<super::model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        records.into_iter().map(FileOperation::try_from).collect()
    }

    /// A lost Task-link reply reconstructs the same metadata-only shared Task.
    pub async fn ensure_file_task(&self, file: &FileOperation) -> Result<()> {
        if file.binding.provider_instance_id != self.provider_instance_id {
            return Err(ComputerError::NotFound);
        }
        let mut read = self
            .query(
                "SELECT * FROM ONLY $execution;",
                vec![("execution", super::record(file.transfer_id()).into_value())],
            )
            .await?;
        let row: Option<super::model::Record> =
            read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let saved = FileOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        if crate::identity::digest(&(&saved.binding, &saved.authority))?
            != crate::identity::digest(&(&file.binding, &file.authority))?
        {
            return Err(ComputerError::StateConflict);
        }
        let file = &saved;
        if file.task_projected() {
            return Err(ComputerError::InvalidState);
        }
        let id = file.transfer_id();
        let reference = file.task_reference()?;
        let pin = TaskRetentionPin::new(format!("computer-file-transfer/{id}"))
            .map_err(|_| ComputerError::Unavailable)?;
        let task = TaskRuntime::new(self.platform.clone(), "computers", "admission")
            .create(CreateTask {
                task_id: file.task_id(),
                owner: file.actor(),
                server: "computers".into(),
                task_type: "computer.file_transfer".into(),
                request: reference.clone(),
                recovery_class: RecoveryClass::ProviderWait,
                idempotency_key: Some(format!("computer-file-transfer/{id}")),
                ttl_ms: None,
                poll_interval_ms: Some(1000),
                retention_pins: BTreeSet::from([pin.clone()]),
            })
            .await
            .map_err(|_| ComputerError::Unavailable)?
            .snapshot;
        if task.task_id != file.task_id()
            || task.owner != file.actor()
            || task.request != reference
            || task.task_type != "computer.file_transfer"
            || task.recovery_class != RecoveryClass::ProviderWait
            || !task.retention_pins.contains(&pin)
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(())
    }

    /// A terminal shared Task acknowledges delivery; its retention pin is released
    /// afterward. Discovery retains a lost pin acknowledgement without recreating work.
    pub async fn acknowledge_file_task(&self, file: &FileOperation) -> Result<()> {
        let (stage, status) = match file.stage() {
            super::FileTransferStage::Failed => ("failed", "failed"),
            super::FileTransferStage::Cancelled => ("cancelled", "cancelled"),
            super::FileTransferStage::Completed => ("completed", "succeeded"),
            _ => return Err(ComputerError::InvalidState),
        };
        self.query(
            include_str!("../../queries/acknowledge_file_task.surql"),
            vec![
                ("execution", super::record(file.transfer_id()).into_value()),
                ("task", file.task_id().record_id().into_value()),
                ("provider", self.provider_instance_id.into_value()),
                ("status", status.into_value()),
                ("stage", stage.into_value()),
            ],
        )
        .await?;
        Ok(())
    }
    pub(super) async fn saved_file(&self, operation: &FileOperation) -> Result<FileOperation> {
        if operation.binding.provider_instance_id != self.provider_instance_id {
            return Err(ComputerError::NotFound);
        }
        let mut response = self
            .query(
                "SELECT * FROM ONLY $transfer;",
                vec![(
                    "transfer",
                    super::record(operation.transfer_id()).into_value(),
                )],
            )
            .await?;
        let row: Option<super::model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let saved = FileOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        if crate::identity::digest(&(&saved.binding, &saved.authority))?
            != crate::identity::digest(&(&operation.binding, &operation.authority))?
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(saved)
    }
}
