//! Transactional preparation of the original WebhookWait submission.
use super::WebhookJournal;
use crate::{DispatchPreparation, TaskDispatch, TaskError};
use chrono::Utc;
use surrealdb::types::RecordId;
use veoveo_platform_store::{TaskOwnerRecord, TaskRequestRecord, task_record_id};
use veoveo_types::TaskId;

impl WebhookJournal {
    /// Commit the owner's dispatch intent before attempting the provider mutation.
    /// An existing receipt can only be reconciled; it never authorizes another send.
    pub async fn prepare_dispatch(
        &self,
        task_id: TaskId,
        dispatch: TaskDispatch,
    ) -> Result<DispatchPreparation, TaskError> {
        let current = self
            .runtime
            .get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        self.check_task(&current)?;
        let admitted = self.runtime.dispatch_contribution(&current, &dispatch)?;
        let query = self
            .runtime
            .store
            .client()
            .query(include_str!(
                "../../../queries/webhooks/prepare_dispatch.surql"
            ))
            .query(admitted.sql())
            .query(include_str!(
                "../../../queries/webhooks/prepare_dispatch_return.surql"
            ))
            .bind(("task", task_record_id(task_id)))
            .bind((
                "server",
                RecordId::new("mcp_server", self.runtime.server.clone()),
            ))
            .bind(("tenant", super::super::tenant_record(&current.owner)?))
            .bind(("task_type", current.task_type.to_string()))
            .bind(("expected_request", TaskRequestRecord::from(&current)))
            .bind((
                "expected_owner_context",
                TaskOwnerRecord::try_from(&current.owner)?,
            ))
            .bind(("expected_updated_at", current.updated_at))
            .bind(("worker", self.runtime.worker_id.clone()))
            .bind((
                "provider_retention_pin",
                crate::TaskRetentionPin::new(format!("provider:{}:webhook", self.provider))
                    .map_err(|error| TaskError::InvalidRecord(error.to_string()))?
                    .to_string(),
            ))
            .bind(("now", Utc::now()));
        let mut response = admitted
            .bind(query, task_id, &current.task_type, current.created_at)
            .await?;
        if let Some(error) =
            veoveo_platform_store::primary_transaction_error(response.take_errors())
        {
            return Err(error.into());
        }
        let newly_prepared: Option<bool> =
            response.take(super::return_slot(response.num_statements())?)?;
        match newly_prepared {
            Some(true) => Ok(DispatchPreparation::NewlyPrepared),
            Some(false) => Ok(DispatchPreparation::AlreadyPrepared),
            None => Err(TaskError::InvalidRecord(
                "Task dispatch receipt result missing".into(),
            )),
        }
    }
}
