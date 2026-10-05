//! Local Task settlement and provider delivery acknowledgment share one transaction.
use super::{AuthenticatedWebhookReceipt, WebhookJournal};
use crate::{TaskError, TaskSettlement, TaskSnapshot, TaskTransition};
use chrono::Utc;
use veoveo_platform_store::{TaskRecord, TaskRequestRecord, TaskResultRecord};

impl WebhookJournal {
    /// Settle the local outcome without changing the provider's first terminal observation.
    pub async fn settle(
        &self,
        receipt: &AuthenticatedWebhookReceipt,
        transition: TaskTransition,
    ) -> Result<TaskSnapshot, TaskError> {
        let task = super::task_id(&receipt.job.task)?;
        let current = self
            .runtime
            .get(task)
            .await?
            .ok_or_else(|| TaskError::NotFound(task.to_string()))?;
        self.check_task(&current)?;
        let external =
            veoveo_platform_store::ProviderJobKey::parse(receipt.job.external_job_id.clone())
                .map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
        let binding = self.binding(&current, external)?;
        if binding.job_id.record_id() != receipt.job.id {
            return Err(TaskError::InvalidRecord(
                "webhook job identity disagrees with Task".into(),
            ));
        }
        let association = self.runtime.provider_association(&current, &binding)?;
        let now = Utc::now();
        let contribution = if current.is_terminal() {
            crate::TaskContribution::none()
        } else {
            let settlement = match &transition {
                TaskTransition::Succeeded { result, .. } => TaskSettlement::Succeeded { result },
                TaskTransition::Failed(failure) => TaskSettlement::Failed { failure },
                TaskTransition::Cancelled => TaskSettlement::Cancelled,
                _ => {
                    return Err(TaskError::InvalidRecord(
                        "webhook settlement requires a terminal outcome".into(),
                    ));
                }
            };
            self.runtime
                .settlement_contribution(&current, settlement, now)?
        };
        let request = TaskRequestRecord {
            input: current.request.clone(),
            status_message: Some(transition.message()),
            ttl_ms: current.ttl_ms,
            poll_interval_ms: current.poll_interval_ms,
        };
        let query = self
            .runtime
            .store
            .client()
            .query(include_str!("../../../queries/webhooks/bind_scope.surql"))
            .query(association.sql())
            .query(include_str!("../../../queries/webhooks/settle.surql"))
            .query(contribution.sql(false)?)
            .query(include_str!(
                "../../../queries/webhooks/settle_return.surql"
            ))
            .bind(("event", receipt.event.id.clone()))
            .bind(("expected_status", current.status))
            .bind(("expected_updated_at", current.updated_at))
            .bind(("next", transition.status()))
            .bind(("next_request", request))
            .bind(("progress", transition.progress(current.progress)))
            .bind(("result", transition.result().map(TaskResultRecord::new)))
            .bind((
                "result_uri",
                transition.result_uri().map(|uri| uri.to_string()),
            ))
            .bind((
                "error",
                transition
                    .failure()
                    .as_ref()
                    .map(super::super::failure_to_record),
            ));
        let query = self.scope(
            query,
            &current,
            &binding,
            receipt.job.provider_payload.clone(),
            now,
        )?;
        let query = association.bind(
            query,
            current.task_id,
            &current.task_type,
            current.created_at,
        );
        let mut response = contribution
            .bind(
                query,
                current.task_id,
                &current.task_type,
                current.created_at,
            )
            .await?;
        if let Some(error) =
            veoveo_platform_store::primary_transaction_error(response.take_errors())
        {
            return Err(error.into());
        }
        let slot = super::return_slot(response.num_statements())?;
        let record: Option<TaskRecord> = response.take(slot)?;
        crate::types::record_to_snapshot(
            record.ok_or_else(|| {
                TaskError::InvalidRecord("webhook settlement result missing".into())
            })?,
        )
    }
    /// Release provider retention only after durable terminal processing and owner billing.
    /// The owner invokes this after its own billing receipt has committed.
    pub async fn release_retention_after_billing(
        &self,
        task: veoveo_types::TaskId,
    ) -> Result<(), TaskError> {
        let current = self
            .runtime
            .get(task)
            .await?
            .ok_or_else(|| TaskError::NotFound(task.to_string()))?;
        self.check_task(&current)?;
        self.runtime
            .store
            .client()
            .query(include_str!(
                "../../../queries/webhooks/release_retention.surql"
            ))
            .bind(("task", veoveo_platform_store::task_record_id(task)))
            .bind((
                "server",
                surrealdb::types::RecordId::new("mcp_server", self.runtime.server.clone()),
            ))
            .bind(("provider", self.provider.to_string()))
            .bind((
                "provider_retention_pin",
                format!("provider:{}:webhook", self.provider),
            ))
            .await?
            .check()?;
        Ok(())
    }

    /// Acknowledge a superseded observation or a Task whose local outcome is already fixed.
    pub async fn acknowledge(
        &self,
        receipt: &AuthenticatedWebhookReceipt,
    ) -> Result<(), TaskError> {
        let task = super::task_id(&receipt.job.task)?;
        let current = self
            .runtime
            .get(task)
            .await?
            .ok_or_else(|| TaskError::NotFound(task.to_string()))?;
        self.check_task(&current)?;
        let external =
            veoveo_platform_store::ProviderJobKey::parse(receipt.job.external_job_id.clone())
                .map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
        let binding = self.binding(&current, external)?;
        let association = self.runtime.provider_association(&current, &binding)?;
        let query = self
            .runtime
            .store
            .client()
            .query(include_str!("../../../queries/webhooks/bind_scope.surql"))
            .query(association.sql())
            .query(include_str!("../../../queries/webhooks/acknowledge.surql"))
            .bind(("event", receipt.event.id.clone()));
        let query = self.scope(
            query,
            &current,
            &binding,
            receipt.job.provider_payload.clone(),
            Utc::now(),
        )?;
        let mut response = association
            .bind(
                query,
                current.task_id,
                &current.task_type,
                current.created_at,
            )
            .await?;
        if let Some(error) =
            veoveo_platform_store::primary_transaction_error(response.take_errors())
        {
            return Err(error.into());
        }
        Ok(())
    }
}
