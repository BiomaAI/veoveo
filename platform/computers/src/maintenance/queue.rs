use super::{MaintenanceOperation, MaintenanceStage, model::MaintenanceRecord, record};
use crate::{ComputerError, ComputersStore, Result};
use surrealdb::types::SurrealValue;
use uuid::Uuid;

impl ComputersStore {
    /// Bounded private worker discovery. Recovery rows do not query the provider.
    pub async fn pending_maintenance(
        &self,
        after: Option<Uuid>,
        limit: u32,
    ) -> Result<Vec<MaintenanceOperation>> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        let mut read = self
            .query(
                include_str!("../../queries/pending_maintenance.surql"),
                vec![
                    ("provider", self.provider_instance_id.into_value()),
                    ("after", after.into_value()),
                    ("limit", i64::from(limit).into_value()),
                ],
            )
            .await?;
        let rows: Vec<MaintenanceRecord> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        rows.into_iter()
            .map(MaintenanceOperation::try_from)
            .collect()
    }
    pub async fn acknowledge_maintenance_task(
        &self,
        operation: &MaintenanceOperation,
    ) -> Result<()> {
        let status = match operation.stage {
            MaintenanceStage::Succeeded => "succeeded",
            MaintenanceStage::Cancelled => "cancelled",
            _ => return Err(ComputerError::InvalidState),
        };
        self.query(
            include_str!("../../queries/acknowledge_task_projection.surql"),
            vec![
                ("operation", record(operation.operation_id).into_value()),
                ("task", operation.task_id().record_id().into_value()),
                ("provider", self.provider_instance_id.into_value()),
                ("status", status.into_value()),
            ],
        )
        .await?;
        Ok(())
    }
}
