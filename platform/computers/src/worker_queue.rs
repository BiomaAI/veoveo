//! Internal bounded worker discovery and recoverable Task delivery.
use crate::{
    ComputerError, ComputersStore, Operation, Result, operation::OperationRecord,
    operation_admission::operation_record,
};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndispatchedOutcome {
    AuthorityDenied,
    CancelledBeforeDispatch,
}

impl ComputersStore {
    /// Trusted worker API. The facade must use owner-scoped operation reads.
    pub async fn operation_for_claim(&self, claimed: &ClaimedTask) -> Result<Operation> {
        Ok(self.worker_operation(claimed).await?.operation)
    }

    /// Iterate all pages before starting another pass; unresolved old work cannot
    /// starve newer operations. Provider queries are never part of discovery.
    pub async fn pending_operations(
        &self,
        after: Option<Uuid>,
        limit: u32,
    ) -> Result<Vec<Operation>> {
        if !(1..=100).contains(&limit) {
            return Err(ComputerError::InvalidInput);
        }
        let mut response = self
            .query(
                include_str!("../queries/pending_operations.surql"),
                vec![
                    ("provider", self.provider_instance_id.into_value()),
                    ("after", after.into_value()),
                    ("limit", i64::from(limit).into_value()),
                ],
            )
            .await?;
        let records: Vec<OperationRecord> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        records.into_iter().map(Operation::try_from).collect()
    }

    /// The shared lease and queued-stage predicate prove that no dispatch ticket
    /// escaped. This path is forbidden after even an uncertain dispatch commit.
    pub async fn abort_undispatched(
        &self,
        claimed: &ClaimedTask,
        outcome: UndispatchedOutcome,
    ) -> Result<Operation> {
        let before = self.worker_operation(claimed).await?.operation;
        let (stage, code) = match outcome {
            UndispatchedOutcome::AuthorityDenied => ("failed", "authority_denied"),
            UndispatchedOutcome::CancelledBeforeDispatch => {
                ("cancelled", "cancelled_before_dispatch")
            }
        };
        self.worker_commit(
            claimed,
            &before,
            ProviderCommit::Observe,
            include_str!("../queries/abort_undispatched.surql"),
            vec![("stage", stage.into_value()), ("code", code.into_value())],
            "computer.operation_undispatched",
        )
        .await?;
        self.operation_for_claim(claimed).await
    }

    /// Call after the shared Task holds its matching terminal projection, and
    /// before acknowledging its retention pin. Discovery repairs a lost pin ack.
    pub async fn acknowledge_task_projection(&self, operation: &Operation) -> Result<()> {
        use crate::OperationStage;
        let status = match operation.stage {
            OperationStage::Succeeded => "succeeded",
            OperationStage::Failed => "failed",
            OperationStage::Cancelled => "cancelled",
            _ => return Err(ComputerError::InvalidState),
        };
        self.query(
            include_str!("../queries/acknowledge_task_projection.surql"),
            vec![
                (
                    "operation",
                    operation_record(operation.operation_id).into_value(),
                ),
                ("task", operation.task_id().record_id().into_value()),
                ("provider", self.provider_instance_id.into_value()),
                ("status", status.into_value()),
            ],
        )
        .await?;
        Ok(())
    }
}
