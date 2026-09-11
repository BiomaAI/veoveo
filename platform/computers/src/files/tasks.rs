use super::FileOperation;
use crate::{ComputerError, ComputersStore, Result};
use std::collections::BTreeSet;
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskRetentionPin, TaskRuntime};

impl ComputersStore {
    pub async fn pending_file_transfers(
        &self,
        after: Option<Uuid>,
        limit: u32,
    ) -> Result<Vec<FileOperation>> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        let mut response=self.query("SELECT * FROM computer_file_transfer WHERE provider_instance_id=$provider AND ($after=NONE OR transfer_id>$after) ORDER BY transfer_id LIMIT $limit;",vec![("provider",self.provider_instance_id.into_value()),("after",after.into_value()),("limit",limit.into_value())]).await?;
        let rows: Vec<super::model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        rows.into_iter().map(FileOperation::try_from).collect()
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
    /// A lost link reply reconstructs the same Task without decrypting file intent.
    pub async fn ensure_file_task(&self, operation: &FileOperation) -> Result<()> {
        let operation = self.saved_file(operation).await?;
        let id = operation.transfer_id();
        let request = operation.task_reference()?;
        let pin = TaskRetentionPin::new(format!("computer-file-transfer/{id}"))
            .map_err(|_| ComputerError::Unavailable)?;
        let task = TaskRuntime::new(self.platform.clone(), "computers", "admission")
            .create(CreateTask {
                task_id: operation.task_id(),
                owner: operation.actor(),
                server: "computers".into(),
                task_type: "computer.file_transfer".into(),
                request: request.clone(),
                recovery_class: RecoveryClass::ProviderWait,
                idempotency_key: Some(format!("computer-file-transfer/{id}")),
                ttl_ms: None,
                poll_interval_ms: Some(1000),
                retention_pins: BTreeSet::from([pin.clone()]),
            })
            .await
            .map_err(|_| ComputerError::Unavailable)?
            .snapshot;
        if task.task_id != operation.task_id()
            || task.owner != operation.actor()
            || task.task_type != "computer.file_transfer"
            || task.request != request
            || task.recovery_class != RecoveryClass::ProviderWait
            || !task.retention_pins.contains(&pin)
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(())
    }
}
