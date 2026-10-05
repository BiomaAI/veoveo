use super::CommandOperation;
use crate::{ComputerError, ComputersStore, Result};
use std::collections::BTreeSet;
use surrealdb::types::SurrealValue;
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskRetentionPin, TaskRuntime};
use veoveo_types::TaskTypeDefinition;

impl ComputersStore {
    /// Trusted worker discovery; public projections use current principal authority.
    pub async fn pending_commands(
        &self,
        after: Option<crate::api::ExecutionId>,
        limit: u32,
    ) -> Result<Vec<CommandOperation>> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        let mut response = self
            .query(
                include_str!("../../queries/commands/tasks/pending_commands.surql"),
                vec![
                    ("provider", self.provider_instance_id.as_uuid().into_value()),
                    (
                        "after",
                        after.map(crate::api::ExecutionId::as_uuid).into_value(),
                    ),
                    ("limit", limit.into_value()),
                ],
            )
            .await?;
        let records: Vec<super::model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        records
            .into_iter()
            .map(CommandOperation::try_from)
            .collect()
    }

    /// A lost Task-link reply reconstructs the same metadata-only shared Task.
    pub async fn ensure_command_task(&self, command: &CommandOperation) -> Result<()> {
        if command.binding.provider_instance_id != self.provider_instance_id {
            return Err(ComputerError::NotFound);
        }
        let mut read = self
            .query(
                include_str!("../../queries/saved_execution.surql"),
                vec![
                    (
                        "journal",
                        super::record(command.execution_id()).into_value(),
                    ),
                    ("provider", self.provider_instance_id.as_uuid().into_value()),
                    (
                        "binding",
                        crate::session_grants::object(&command.binding)?.into_value(),
                    ),
                    (
                        "authority",
                        crate::session_grants::object(&command.authority)?.into_value(),
                    ),
                ],
            )
            .await?;
        let row: Option<super::model::Record> =
            read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let saved = CommandOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        let command = &saved;
        if command.task_projected() {
            return Err(ComputerError::InvalidState);
        }
        let id = command.execution_id();
        let reference = command.task_reference()?;
        let pin = TaskRetentionPin::new(format!("computer-execution/{id}"))
            .map_err(|_| ComputerError::Unavailable)?;
        let task = TaskRuntime::new(self.platform.clone(), "computers", "admission")
            .create(CreateTask {
                task_id: command.task_id(),
                owner: command.actor(),
                server: "computers".into(),
                task_type: crate::api::ComputerTaskKind::Execution.name(),
                request: reference.clone(),
                recovery_class: RecoveryClass::ProviderWait,
                idempotency_key: Some(format!("computer-execution/{id}")),
                ttl_ms: None,
                poll_interval_ms: Some(1000),
                retention_pins: BTreeSet::from([pin.clone()]),
            })
            .await
            .map_err(|_| ComputerError::Unavailable)?
            .snapshot;
        if task.task_id != command.task_id()
            || task.owner != command.actor()
            || task.request != reference
            || task.task_type != crate::api::ComputerTaskKind::Execution.name()
            || task.recovery_class != RecoveryClass::ProviderWait
            || !task.retention_pins.contains(&pin)
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(())
    }

    /// A terminal shared Task acknowledges delivery; its retention pin is released
    /// afterward. Discovery retains a lost pin acknowledgement without recreating work.
    pub async fn acknowledge_command_task(&self, command: &CommandOperation) -> Result<()> {
        let (stage, status) = match command.stage() {
            super::CommandStage::Failed => ("failed", "failed"),
            super::CommandStage::Cancelled => ("cancelled", "cancelled"),
            super::CommandStage::Completed => ("completed", "succeeded"),
            _ => return Err(ComputerError::InvalidState),
        };
        self.query(
            include_str!("../../queries/acknowledge_command_task.surql"),
            vec![
                (
                    "execution",
                    super::record(command.execution_id()).into_value(),
                ),
                ("task", task_record_id(command.task_id()).into_value()),
                ("provider", self.provider_instance_id.as_uuid().into_value()),
                ("status", status.into_value()),
                ("stage", stage.into_value()),
            ],
        )
        .await?;
        Ok(())
    }
}
