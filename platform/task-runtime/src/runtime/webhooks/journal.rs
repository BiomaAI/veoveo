//! Provider journal writes coupled to registered immutable owner receipts.
use super::{AuthenticatedWebhookReceipt, WebhookJournal, WebhookTerminal};
use crate::{TaskError, TaskSnapshot};
use chrono::{DateTime, Utc};
use surrealdb::{
    Connection,
    method::Query,
    types::{RecordId, SurrealValue, Value},
};
use uuid::Uuid;
use veoveo_platform_store::{
    OpenObject, ProviderEventId, ProviderEventKey, ProviderEventRecord, ProviderJobId,
    ProviderJobKey, ProviderJobRecord, ProviderJobState, TaskOwnerRecord, TaskRequestRecord,
    WebhookJobBinding, deterministic_tenant_id, task_record_id,
};
use veoveo_types::{ExtensionName, TaskId};

const JOURNAL_IDS: Uuid = Uuid::from_u128(0x664be62c_e4a4_5d53_b2c1_81bcaad42d20);

#[derive(SurrealValue)]
pub(super) struct Receipt {
    event: ProviderEventRecord,
    job: ProviderJobRecord,
    inserted: bool,
    authoritative: bool,
}
impl WebhookJournal {
    pub(super) fn binding(
        &self,
        current: &TaskSnapshot,
        external: ProviderJobKey,
    ) -> Result<WebhookJobBinding, TaskError> {
        let tenant = deterministic_tenant_id(current.owner.tenant_key())?;
        let key = serde_json::to_vec(&(
            tenant.to_string(),
            self.provider.as_str(),
            external.as_str(),
        ))?;
        Ok(WebhookJobBinding {
            job_id: ProviderJobId::from_uuid(Uuid::new_v5(&JOURNAL_IDS, &key)),
            task_id: current.task_id,
            server: ExtensionName::parse(self.runtime.server.clone())
                .map_err(|error| TaskError::InvalidRecord(error.to_string()))?,
            tenant,
            task_type: current.task_type.clone(),
            provider: self.provider.clone(),
            external_job_id: external,
        })
    }
    pub(super) fn scope<'q, C: Connection>(
        &self,
        query: Query<'q, C>,
        current: &TaskSnapshot,
        binding: &WebhookJobBinding,
        payload: OpenObject,
        now: DateTime<Utc>,
    ) -> Result<Query<'q, C>, TaskError> {
        let job = ProviderJobRecord {
            id: binding.job_id.record_id(),
            tenant: binding.tenant.record_id(),
            task: task_record_id(current.task_id),
            provider: binding.provider.to_string(),
            external_job_id: binding.external_job_id.to_string(),
            state: ProviderJobState::Waiting,
            provider_payload: payload.clone(),
            submitted_at: now,
            updated_at: now,
            completed_at: None,
            cancellation_receipt: None,
            terminal_event: None,
            observed_at: None,
        };
        Ok(query
            .bind(("task", task_record_id(current.task_id)))
            .bind((
                "server",
                RecordId::new("mcp_server", self.runtime.server.clone()),
            ))
            .bind(("tenant", binding.tenant.record_id()))
            .bind(("task_type", current.task_type.to_string()))
            .bind(("expected_request", TaskRequestRecord::from(current)))
            .bind((
                "expected_owner_context",
                TaskOwnerRecord::try_from(&current.owner)?,
            ))
            .bind(("job", binding.job_id.record_id()))
            .bind(("job_content", job))
            .bind(("payload", payload))
            .bind(("provider", self.provider.to_string()))
            .bind(("external_job_id", binding.external_job_id.to_string()))
            .bind(("now", now)))
    }
    /// Record the definitive submission identity without overriding an earlier callback.
    pub async fn bind_submission(
        &self,
        task: TaskId,
        external: ProviderJobKey,
        payload: OpenObject,
        waiting_message: String,
    ) -> Result<ProviderJobRecord, TaskError> {
        let current = self
            .runtime
            .get(task)
            .await?
            .ok_or_else(|| TaskError::NotFound(task.to_string()))?;
        self.check_task(&current)?;
        let binding = self.binding(&current, external)?;
        let association = self.runtime.provider_association(&current, &binding)?;
        let query = self
            .runtime
            .store
            .client()
            .query(include_str!("../../../queries/webhooks/bind_scope.surql"))
            .query(association.sql())
            .query(include_str!(
                "../../../queries/webhooks/bind_submission.surql"
            ))
            .bind(("waiting_message", waiting_message));
        let query = self.scope(query, &current, &binding, payload, Utc::now())?;
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
        let job: Option<ProviderJobRecord> = response.take(slot)?;
        job.ok_or_else(|| TaskError::InvalidRecord("provider job binding result missing".into()))
    }
    /// Accept one authenticated terminal observation. Owner-specific callback verification precedes this API.
    pub async fn receive_authenticated(
        &self,
        task: TaskId,
        external: ProviderJobKey,
        event: ProviderEventKey,
        terminal: Option<WebhookTerminal>,
        payload: OpenObject,
        signing_key_id: Option<String>,
    ) -> Result<AuthenticatedWebhookReceipt, TaskError> {
        let current = self
            .runtime
            .get(task)
            .await?
            .ok_or_else(|| TaskError::NotFound(task.to_string()))?;
        self.check_task(&current)?;
        let binding = self.binding(&current, external)?;
        let association = self.runtime.provider_association(&current, &binding)?;
        let event_key = serde_json::to_vec(&(
            binding.tenant.to_string(),
            self.provider.as_str(),
            event.as_str(),
        ))?;
        let event_id = ProviderEventId::from_uuid(Uuid::new_v5(&JOURNAL_IDS, &event_key));
        let now = Utc::now();
        let content = ProviderEventRecord {
            id: event_id.record_id(),
            tenant: binding.tenant.record_id(),
            provider_job: binding.job_id.record_id(),
            provider: self.provider.to_string(),
            event_id: event.to_string(),
            signing_key_id,
            payload: payload.clone(),
            received_at: now,
            processed_at: None,
            processing_error: None,
        };
        let state = match terminal {
            Some(WebhookTerminal::Succeeded) => ProviderJobState::Succeeded,
            Some(WebhookTerminal::Failed) => ProviderJobState::Failed,
            Some(WebhookTerminal::Cancelled) => ProviderJobState::Cancelled,
            None => ProviderJobState::Waiting,
        };
        let query = self
            .runtime
            .store
            .client()
            .query(include_str!("../../../queries/webhooks/bind_scope.surql"))
            .query(association.sql())
            .query(include_str!("../../../queries/webhooks/receive.surql"))
            .bind(("event", event_id.record_id()))
            .bind(("event_content", content))
            .bind(("event_id", event.to_string()))
            .bind(("terminal_state", state))
            .bind(("is_terminal", terminal.is_some()));
        let query = self.scope(query, &current, &binding, payload, now)?;
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
        let value: Value = response.take(slot)?;
        let receipt = Receipt::from_value(value)
            .map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
        Ok(AuthenticatedWebhookReceipt {
            event: receipt.event,
            job: receipt.job,
            inserted: receipt.inserted,
            authoritative: receipt.authoritative,
        })
    }
}

impl From<Receipt> for AuthenticatedWebhookReceipt {
    fn from(value: Receipt) -> Self {
        Self {
            event: value.event,
            job: value.job,
            inserted: value.inserted,
            authoritative: value.authoritative,
        }
    }
}
