use super::CommandOperation;
use crate::{ComputerError, ComputersStore, Result};
use std::collections::BTreeSet;
use surrealdb::types::SurrealValue;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskRetentionPin, TaskRuntime};

impl ComputersStore {
    /// Trusted worker discovery; public projections use current principal authority.
    pub async fn pending_commands(
        &self,
        after: Option<uuid::Uuid>,
        limit: u32,
    ) -> Result<Vec<CommandOperation>> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        let mut response = self
            .query(
                "SELECT * FROM computer_execution WHERE provider_instance_id = $provider
             AND ($after = NONE OR execution_id > $after)
             AND (stage != 'recovery_required' OR task.status = NONE OR task.status IN ['queued', 'running'])
             AND (task_projected_at = NONE OR task.retention_pins CONTAINS string::concat('computer-execution/', <string>execution_id))
             ORDER BY execution_id LIMIT $limit;",
                vec![
                    ("provider", self.provider_instance_id.into_value()),
                    ("after", after.into_value()),
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
                "SELECT * FROM ONLY $execution;",
                vec![(
                    "execution",
                    super::record(command.execution_id()).into_value(),
                )],
            )
            .await?;
        let row: Option<super::model::Record> =
            read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let saved = CommandOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        if crate::identity::digest(&(&saved.binding, &saved.authority))?
            != crate::identity::digest(&(&command.binding, &command.authority))?
        {
            return Err(ComputerError::StateConflict);
        }
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
                task_type: "computer.execution".into(),
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
            || task.task_type != "computer.execution"
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
                ("task", command.task_id().record_id().into_value()),
                ("provider", self.provider_instance_id.into_value()),
                ("status", status.into_value()),
                ("stage", stage.into_value()),
            ],
        )
        .await?;
        Ok(())
    }
}
