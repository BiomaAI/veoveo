use super::{MaintenanceOperation, MaintenanceStage, model::MaintenanceRecord, record};
use crate::{ComputerError, ComputersStore, Result};
use surrealdb::types::SurrealValue;
use veoveo_platform_store::task_record_id;

impl ComputersStore {
    /// Bounded private worker discovery. Recovery rows do not query the provider.
    pub async fn pending_maintenance(
        &self,
        after: Option<veoveo_types::TaskId>,
        limit: u32,
    ) -> Result<Vec<MaintenanceOperation>> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        let mut read = self
            .query(
                include_str!("../../queries/pending_maintenance.surql"),
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
                        after.map(veoveo_types::TaskId::as_uuid).into_value(),
                    ),
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
                ("task", task_record_id(operation.task_id()).into_value()),
                ("provider", self.provider_instance_id.as_uuid().into_value()),
                ("status", status.into_value()),
                (
                    "server",
                    surrealdb::types::RecordId::new("mcp_server", "computers").into_value(),
                ),
                (
                    "task_tenant",
                    crate::identity::task_tenant(&operation.actor)?.into_value(),
                ),
                (
                    "task_types",
                    vec![crate::api::ComputerTaskKind::Maintenance.name().to_string()].into_value(),
                ),
            ],
        )
        .await?;
        Ok(())
    }
}

use veoveo_types::TaskTypeDefinition;
