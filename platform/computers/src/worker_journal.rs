//! Durable worker state, independent of the selected provider SDK.
use crate::{
    ComputerError, ComputersStore, Operation, Result, model::computer_record,
    operation::OperationRecord, operation_admission::operation_record,
};
use chrono::{DateTime, Utc};
use std::time::{Duration, Instant};
use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_task_runtime::{ClaimedTask, ProviderCommit, TaskError, TaskRuntime};

pub(crate) struct ClockedOperation {
    pub operation: Operation,
    pub database_time: DateTime<Utc>,
    pub read_started: Instant,
}
impl ClockedOperation {
    pub fn remaining(&self) -> Duration {
        self.operation
            .observation_deadline
            .and_then(|deadline| (deadline - self.database_time).to_std().ok())
            .unwrap_or_default()
            .saturating_sub(self.read_started.elapsed())
    }
}

impl ComputersStore {
    pub(crate) async fn worker_operation(&self, claimed: &ClaimedTask) -> Result<ClockedOperation> {
        if claimed.snapshot.server != "computers" {
            return Err(ComputerError::InvalidInput);
        }
        let id = Uuid::parse_str(&claimed.snapshot.task_id.to_string())
            .map_err(|_| ComputerError::InvalidInput)?;
        let started = Instant::now();
        let mut response = self
            .query(
                "SELECT * FROM ONLY $operation; RETURN time::now();",
                vec![("operation", operation_record(id).into_value())],
            )
            .await?;
        let record: Option<OperationRecord> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let database_time: Option<DateTime<Utc>> =
            response.take(1).map_err(|_| ComputerError::Unavailable)?;
        let database_time = database_time.ok_or(ComputerError::Unavailable)?;
        let operation = Operation::try_from(record.ok_or(ComputerError::NotFound)?)?;
        if operation.task_id() != claimed.snapshot.task_id
            || operation.actor != claimed.snapshot.owner
            || operation.provider_instance_id != self.provider_instance_id
            || claimed.snapshot.request
                != serde_json::json!({"computerId": operation.computer_id, "operationId": operation.operation_id})
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(ClockedOperation {
            operation,
            database_time,
            read_started: started,
        })
    }

    pub(crate) async fn worker_commit(
        &self,
        claimed: &ClaimedTask,
        operation: &Operation,
        kind: ProviderCommit,
        body: &'static str,
        mut bindings: Vec<(&'static str, Value)>,
        event: crate::audit::LifecycleTransition,
    ) -> Result<()> {
        bindings.extend([
            (
                "operation",
                operation_record(operation.operation_id).into_value(),
            ),
            ("operation_id", operation.operation_id.into_value()),
            (
                "computer",
                computer_record(operation.computer_id).into_value(),
            ),
            ("provider", self.provider_instance_id.into_value()),
        ]);
        bindings.push(crate::audit::binding(
            &operation.execution_authority,
            operation.computer_id,
            event.transition(operation),
        )?);
        // Construct against this store, so a caller cannot accidentally commit its
        // Task guard against another installation's database.
        TaskRuntime::new(self.platform.clone(), "computers", &claimed.lease_owner)
            .commit_provider_journal(claimed, kind, body, bindings)
            .await
            .map_err(|error| match error {
                TaskError::Conflict(_) | TaskError::LeaseHeld(_) => ComputerError::StateConflict,
                _ => ComputerError::Unavailable,
            })
    }
}
