//! Trusted-service journal recovery reads. Task identity is admitted in SQL before limits.
use super::{AuthenticatedWebhookReceipt, WebhookJournal};
use crate::TaskError;
use surrealdb::types::RecordId;
use veoveo_platform_store::{ProviderJobRecord, task_record_id};
use veoveo_types::{TaskId, TaskTypeName};

impl WebhookJournal {
    pub async fn pending(
        &self,
        operations: &[TaskTypeName],
        limit: usize,
    ) -> Result<Vec<AuthenticatedWebhookReceipt>, TaskError> {
        if operations.is_empty() || operations.len() > 32 || !(1..=1000).contains(&limit) {
            return Err(TaskError::InvalidPageQuery);
        }
        for operation in operations {
            self.runtime.require_contribution(operation)?;
        }
        let mut response = self
            .runtime
            .store
            .client()
            .query(include_str!("../../../queries/webhooks/pending.surql"))
            .bind(("provider", self.provider.to_string()))
            .bind((
                "server",
                RecordId::new("mcp_server", self.runtime.server.clone()),
            ))
            .bind((
                "task_types",
                operations
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind(("limit", limit))
            .await?
            .check()?;
        let rows: Vec<super::journal::Receipt> = response.take(0)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
    pub async fn job_for_task(&self, task: TaskId) -> Result<Option<ProviderJobRecord>, TaskError> {
        let current = self
            .runtime
            .get(task)
            .await?
            .ok_or_else(|| TaskError::NotFound(task.to_string()))?;
        self.check_task(&current)?;
        let mut response = self
            .runtime
            .store
            .client()
            .query(include_str!("../../../queries/webhooks/job_for_task.surql"))
            .bind(("task", task_record_id(task)))
            .bind(("provider", self.provider.to_string()))
            .bind((
                "server",
                RecordId::new("mcp_server", self.runtime.server.clone()),
            ))
            .bind(("tenant", super::super::tenant_record(&current.owner)?))
            .bind(("task_types", vec![current.task_type.to_string()]))
            .await?
            .check()?;
        let mut rows: Vec<ProviderJobRecord> = response.take(0)?;
        if rows.len() > 1 {
            return Err(TaskError::InvalidRecord(
                "multiple provider identities for one Task".into(),
            ));
        }
        Ok(rows.pop())
    }
    pub async fn processing_error(
        &self,
        receipt: &AuthenticatedWebhookReceipt,
        error: String,
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
            .query(include_str!(
                "../../../queries/webhooks/processing_error.surql"
            ))
            .bind(("event", receipt.event.id.clone()))
            .bind(("error", error));
        let query = self.scope(
            query,
            &current,
            &binding,
            receipt.job.provider_payload.clone(),
            chrono::Utc::now(),
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

impl WebhookJournal {
    /// Cancellation acknowledgments record the request outcome, never a terminal observation.
    pub async fn record_cancellation(
        &self,
        task: TaskId,
        receipt: veoveo_platform_store::OpenObject,
    ) -> Result<ProviderJobRecord, TaskError> {
        let current = self
            .runtime
            .get(task)
            .await?
            .ok_or_else(|| TaskError::NotFound(task.to_string()))?;
        self.check_task(&current)?;
        let job = self.job_for_task(task).await?.ok_or_else(|| {
            TaskError::InvalidRecord("provider job missing for cancellation receipt".into())
        })?;
        let external = veoveo_platform_store::ProviderJobKey::parse(job.external_job_id.clone())
            .map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
        let binding = self.binding(&current, external)?;
        let association = self.runtime.provider_association(&current, &binding)?;
        let query = self
            .runtime
            .store
            .client()
            .query(include_str!("../../../queries/webhooks/bind_scope.surql"))
            .query(association.sql())
            .query(include_str!(
                "../../../queries/webhooks/cancellation_receipt.surql"
            ))
            .bind(("cancellation_receipt", receipt));
        let query = self.scope(
            query,
            &current,
            &binding,
            job.provider_payload,
            chrono::Utc::now(),
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
        let slot = super::return_slot(response.num_statements())?;
        let updated: Option<ProviderJobRecord> = response.take(slot)?;
        updated.ok_or_else(|| {
            TaskError::InvalidRecord("provider cancellation receipt readback missing".into())
        })
    }
}
