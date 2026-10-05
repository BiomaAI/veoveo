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
    ) -> Result<BTreeMap<crate::api::ComputerId, ComputerExecution>> {
        if computers.len() > 100 {
            return Err(ComputerError::InvalidInput);
        }
        for computer in computers {
            permits(&computer.owner, owner)?;
        }
        self.read_active_executions(computers).await
    }
    pub async fn active_executions_for_access(
        &self,
        access: &[crate::ComputerReadAccess],
    ) -> Result<BTreeMap<crate::api::ComputerId, ComputerExecution>> {
        if access.len() > 100 {
            return Err(ComputerError::InvalidInput);
        }
        let computers: Vec<_> = access
            .iter()
            .map(|access| access.computer().cloned())
            .collect::<Result<_>>()?;
        let active = self.read_active_executions(&computers).await?;
        for access in access {
            access.computer()?;
        }
        Ok(active)
    }
    async fn read_active_executions(
        &self,
        computers: &[Computer],
    ) -> Result<BTreeMap<crate::api::ComputerId, ComputerExecution>> {
        if computers.is_empty() {
            return Ok(BTreeMap::new());
        }
        let slots: Vec<_> = computers
            .iter()
            .map(|c| crate::commands::slot(c.computer_id))
            .collect();
        let mut read = self
            .query(
                include_str!("../queries/active_execution/read_active_executions.surql"),
                vec![("slots", slots.into_value())],
            )
            .await?;
        let rows: Vec<Slot> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let mut active = BTreeMap::new();
        for row in rows {
            let computer_id = crate::api::ComputerId::try_from(row.computer_id)
                .map_err(|_| ComputerError::Unavailable)?;
            if row.id != crate::commands::slot(computer_id)
                || row.target_computer != row.computer_id
                || !computers.iter().any(|c| c.computer_id == computer_id)
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
                "computer_execution" => ComputerExecution::Command {
                    task_id: veoveo_types::TaskId::from_uuid(task_id),
                },
                "computer_file_transfer" => ComputerExecution::File {
                    task_id: veoveo_types::TaskId::from_uuid(task_id),
                },
                _ => return Err(ComputerError::Unavailable),
            };
            if active.insert(computer_id, work).is_some() {
                return Err(ComputerError::Unavailable);
            }
        }
        Ok(active)
    }
}
