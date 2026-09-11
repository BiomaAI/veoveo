//! Explicit domain-authorized recovery under an unchanged observation claim.
use crate::{ClaimedTask, ProviderCommit, TaskError, TaskRuntime, TaskSnapshot, TaskStatus};
use chrono::{DateTime, Utc};
use surrealdb::types::{SurrealValue, Value};

impl TaskRuntime {
    /// Resume a paused provider journal and its Task in one transaction.
    ///
    /// The trusted domain body must authorize and durably record an idempotent
    /// recovery request, compare its paused epoch, and retain its resource fence
    /// and original dispatch identities. This helper grants no provider effect.
    /// Cancellation requires the exact pending timestamp; ordinary transitions
    /// cannot withdraw it. A subsequent cancellation wins normally. Lost database
    /// replies require looking up the domain request, never blind resubmission.
    pub async fn resume_provider_journal(
        &self,
        claimed: &ClaimedTask,
        acknowledged_cancellation: Option<DateTime<Utc>>,
        body: &'static str,
        mut bindings: Vec<(&'static str, Value)>,
    ) -> Result<TaskSnapshot, TaskError> {
        let current = &claimed.snapshot;
        let pending = (current.status == TaskStatus::CancelRequested)
            .then_some(current.cancel_requested_at)
            .flatten();
        if !matches!(
            current.status,
            TaskStatus::Queued
                | TaskStatus::Running
                | TaskStatus::Waiting
                | TaskStatus::CancelRequested
        ) || current.status == TaskStatus::CancelRequested && pending.is_none()
            || acknowledged_cancellation != pending
            || bindings
                .iter()
                .any(|(name, _)| name.starts_with("_resume_"))
        {
            return Err(TaskError::Conflict(current.task_id.to_string()));
        }
        let now = Utc::now();
        let mut resumed = current.clone();
        resumed.status = TaskStatus::Waiting;
        resumed.status_message = Some("Recovery explicitly resumed".into());
        resumed.updated_at = now;
        // The old cancellation remains historical evidence. Pending intent is
        // represented by Task status, and a new cancellation records a new epoch.
        let envelope = crate::types::RequestEnvelope {
            input: current.request.clone(),
            owner: current.owner.clone(),
            status_message: resumed.status_message.clone(),
            ttl_ms: current.ttl_ms,
            poll_interval_ms: current.poll_interval_ms,
        }
        .into_open_object()?;
        bindings.extend([
            ("_resume_status", current.status.into_value()),
            ("_resume_updated", current.updated_at.into_value()),
            (
                "_resume_cancelled",
                current.cancel_requested_at.into_value(),
            ),
            ("_resume_now", now.into_value()),
            ("_resume_request", envelope.into_value()),
            (
                "_resume_event",
                crate::runtime::task_event(&resumed, "task.recovery_resumed")?.into_value(),
            ),
        ]);
        let body = format!(
            "IF $_provider_guard.status != $_resume_status \
             OR $_provider_guard.updated_at != $_resume_updated \
             OR $_provider_guard.cancel_requested_at != $_resume_cancelled \
             {{ THROW 'provider_lease_lost'; }}; \
             {body}\n\
             UPDATE ONLY $_provider_task SET status = 'waiting', request = $_resume_request, updated_at = $_resume_now; \
             CREATE outbox_event CONTENT $_resume_event;"
        );
        self.commit_provider_body(claimed, ProviderCommit::Observe, &body, bindings)
            .await?;
        Ok(resumed)
    }
}
