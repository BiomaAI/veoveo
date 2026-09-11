//! One bounded metadata read for the visible collection's shared execution slots.
use crate::{
    Computer, ComputerError, ComputersStore, Result, api::ComputerExecution, identity::permits,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use uuid::Uuid;
use veoveo_task_runtime::TaskOwner;

#[derive(Deserialize, SurrealValue)]
struct Slot {
    id: RecordId,
    computer_id: Uuid,
    execution: RecordId,
    target_computer: Uuid,
}

impl ComputersStore {
    pub async fn active_executions(
        &self,
        owner: &TaskOwner,
        computers: &[Computer],
    ) -> Result<BTreeMap<Uuid, ComputerExecution>> {
        if computers.len() > 100 {
            return Err(ComputerError::InvalidInput);
        }
        for computer in computers {
            permits(&computer.owner, owner)?;
        }
        if computers.is_empty() {
            return Ok(BTreeMap::new());
        }
        let slots: Vec<_> = computers
            .iter()
            .map(|c| crate::commands::slot(c.computer_id))
            .collect();
        let mut read = self.query("SELECT id, computer_id, execution, execution.computer_id AS target_computer FROM $slots;",
            vec![("slots", slots.into_value())]).await?;
        let rows: Vec<Slot> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let mut active = BTreeMap::new();
        for row in rows {
            if row.id != crate::commands::slot(row.computer_id)
                || row.target_computer != row.computer_id
                || !computers.iter().any(|c| c.computer_id == row.computer_id)
            {
                return Err(ComputerError::Unavailable);
            }
            let RecordIdKey::Uuid(task) = row.execution.key else {
                return Err(ComputerError::Unavailable);
            };
            let task_id = task.into_inner();
            if task_id.get_version_num() != 7 {
                return Err(ComputerError::Unavailable);
            }
            let work = match row.execution.table.as_str() {
                "computer_execution" => ComputerExecution::Command { task_id },
                "computer_file_transfer" => ComputerExecution::File { task_id },
                _ => return Err(ComputerError::Unavailable),
            };
            if active.insert(row.computer_id, work).is_some() {
                return Err(ComputerError::Unavailable);
            }
        }
        Ok(active)
    }
}
