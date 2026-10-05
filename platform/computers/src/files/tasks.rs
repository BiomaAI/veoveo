use super::FileOperation;
use crate::{ComputerError, ComputersStore, Result};
use std::collections::BTreeSet;
use surrealdb::types::SurrealValue;
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskRetentionPin, TaskRuntime};
use veoveo_types::TaskTypeDefinition;

impl ComputersStore {
    /// Trusted worker discovery; public projections use current principal authority.
    pub async fn pending_file_transfers(
        &self,
        after: Option<crate::api::FileTransferId>,
        limit: u32,
    ) -> Result<Vec<FileOperation>> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        let mut response = self
            .query(
                include_str!("../../queries/files/tasks/pending_file_transfers.surql"),
                vec![
                    ("provider", self.provider_instance_id.as_uuid().into_value()),
                    (
                        "enterprise",
                        veoveo_platform_store::deterministic_enterprise_id()
                            .record_id()
                            .into_value(),
                    ),
                    (
                        "server",
                        surrealdb::types::RecordId::new("mcp_server", "computers").into_value(),
                    ),
                    (
                        "after",
                        after.map(crate::api::FileTransferId::as_uuid).into_value(),
                    ),
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
        let saved = self.saved_file(file).await?;
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
                task_type: crate::api::ComputerTaskKind::FileTransfer.name(),
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
            || task.task_type != crate::api::ComputerTaskKind::FileTransfer.name()
            || task.recovery_class != RecoveryClass::ProviderWait
            || !task.retention_pins.contains(&pin)
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(())
    }

    /// A terminal shared Task acknowledges delivery; its retention pin is released
    /// afterward. Discovery retains a lost pin acknowledgement without recreating work.
    pub async fn acknowledge_file_task(
        &self,
        file: &FileOperation,
        expected_output: Option<&serde_json::Value>,
    ) -> Result<()> {
        let (stage, status) = match file.stage() {
            super::FileTransferStage::Failed => ("failed", "failed"),
            super::FileTransferStage::Cancelled => ("cancelled", "cancelled"),
            super::FileTransferStage::Completed => ("completed", "succeeded"),
            _ => return Err(ComputerError::InvalidState),
        };
        let expected_domain = match file.outcome().ok_or(ComputerError::InvalidState)? {
            super::FileOutcome::Completed(result) => {
                if expected_output.is_none() {
                    return Err(ComputerError::InvalidInput);
                }
                crate::session_grants::object(&result)?.into_value()
            }
            _ => {
                if expected_output.is_some() {
                    return Err(ComputerError::InvalidInput);
                }
                surrealdb::types::Value::None
            }
        };
        let tenant = crate::identity::task_tenant(&file.actor())?;
        self.query(
            include_str!("../../queries/acknowledge_file_task.surql"),
            vec![
                ("execution", super::record(file.transfer_id()).into_value()),
                ("task", task_record_id(file.task_id()).into_value()),
                ("provider", self.provider_instance_id.as_uuid().into_value()),
                ("status", status.into_value()),
                ("stage", stage.into_value()),
                (
                    "server",
                    surrealdb::types::RecordId::new("mcp_server", "computers").into_value(),
                ),
                ("task_tenant", tenant.into_value()),
                (
                    "task_types",
                    vec![
                        crate::api::ComputerTaskKind::FileTransfer
                            .name()
                            .to_string(),
                    ]
                    .into_value(),
                ),
                (
                    "expected_result",
                    crate::identity::task_output(expected_output)?,
                ),
                ("expected_domain", expected_domain),
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
                include_str!("../../queries/saved_execution.surql"),
                vec![
                    (
                        "journal",
                        super::record(operation.transfer_id()).into_value(),
                    ),
                    ("provider", self.provider_instance_id.as_uuid().into_value()),
                    ("binding", operation.binding.clone().into_value()),
                    ("authority", operation.authority.clone().into_value()),
                ],
            )
            .await?;
        let row: Option<super::model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let saved = FileOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        Ok(saved)
    }
}
