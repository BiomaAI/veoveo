//! Recovery preserves each declared completion profile.
use crate::{
    TaskRuntime,
    runtime::{recovery_result, status_name, task_event},
    types::{
        RecoveryClass, RecoveryReport, RequestEnvelope, TaskError, TaskFailure, TaskSnapshot,
        TaskTransition, failure_to_open_object, record_to_snapshot,
    },
};
use chrono::Utc;
use veoveo_platform_store::{TaskRecord, TaskStatus as StoreTaskStatus};
impl TaskRuntime {
    pub async fn recover(&self) -> Result<RecoveryReport, TaskError> {
        let mut report = RecoveryReport::default();
        let tasks = self.list().await?;
        for task in tasks {
            if task
                .lease_expires_at
                .is_some_and(|expiry| expiry > Utc::now())
            {
                continue;
            }
            if task.recovery_class == RecoveryClass::ProviderWait && !task.is_terminal() {
                report.provider_waiting.push(task);
                continue;
            }
            match task.status {
                StoreTaskStatus::Queued => {
                    if task.recovery_class == RecoveryClass::WebhookWait {
                        if let Some(waiting) = recovery_result(self.force_waiting(&task).await)? {
                            report.webhook_waiting.push(waiting);
                        }
                    } else {
                        report.resumable.push(task);
                    }
                }
                StoreTaskStatus::CancelRequested => {
                    if let Some(cancelled) = recovery_result(
                        self.transition_if_current(&task, TaskTransition::Cancelled)
                            .await,
                    )? {
                        report.cancelled.push(cancelled);
                    }
                }
                StoreTaskStatus::Running | StoreTaskStatus::Waiting => match task.recovery_class {
                    RecoveryClass::Resume => {
                        if let Some(reset) = recovery_result(self.reset_for_recovery(&task).await)?
                        {
                            report.resumable.push(reset);
                        }
                    }
                    RecoveryClass::WebhookWait => {
                        let waiting = if task.status == StoreTaskStatus::Waiting {
                            Some(task)
                        } else {
                            recovery_result(self.force_waiting(&task).await)?
                        };
                        if let Some(waiting) = waiting {
                            report.webhook_waiting.push(waiting);
                        }
                    }
                    RecoveryClass::ProviderWait => unreachable!("handled before status recovery"),
                    RecoveryClass::InterruptedIndeterminate => {
                        if let Some(failed) = recovery_result(
                            self.force_failed(&task, TaskFailure::interrupted_indeterminate())
                                .await,
                        )? {
                            report.failed_indeterminate.push(failed);
                        }
                    }
                },
                StoreTaskStatus::Succeeded
                | StoreTaskStatus::Failed
                | StoreTaskStatus::Cancelled => {}
            }
        }
        Ok(report)
    }

    async fn reset_for_recovery(&self, task: &TaskSnapshot) -> Result<TaskSnapshot, TaskError> {
        self.force_status(
            task,
            StoreTaskStatus::Queued,
            "reclaimed after process restart",
            None,
        )
        .await
    }

    async fn force_waiting(&self, task: &TaskSnapshot) -> Result<TaskSnapshot, TaskError> {
        self.force_status(
            task,
            StoreTaskStatus::Waiting,
            "waiting for provider webhook",
            None,
        )
        .await
    }

    async fn force_failed(
        &self,
        task: &TaskSnapshot,
        failure: TaskFailure,
    ) -> Result<TaskSnapshot, TaskError> {
        let message = failure.message.clone();
        self.force_status(task, StoreTaskStatus::Failed, &message, Some(failure))
            .await
    }

    async fn force_status(
        &self,
        task: &TaskSnapshot,
        status: StoreTaskStatus,
        message: &str,
        failure: Option<TaskFailure>,
    ) -> Result<TaskSnapshot, TaskError> {
        let now = Utc::now();
        let envelope = RequestEnvelope {
            input: task.request.clone(),
            owner: task.owner.clone(),
            status_message: Some(message.to_owned()),
            ttl_ms: task.ttl_ms,
            poll_interval_ms: task.poll_interval_ms,
        };
        let terminal = status == StoreTaskStatus::Failed;
        let mut event_snapshot = task.clone();
        event_snapshot.status = status;
        event_snapshot.status_message = Some(message.to_owned());
        event_snapshot.error = failure.clone();
        event_snapshot.lease_owner = None;
        event_snapshot.lease_expires_at = None;
        event_snapshot.completed_at = terminal.then_some(now);
        event_snapshot.updated_at = now;
        let event = task_event(&event_snapshot, &format!("task.{}", status_name(status)))?;
        let mut response = self
            .platform_store()
            .client()
            .query(
                "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = $status, request = $request, error = $error, lease_owner = NONE, lease_expires_at = NONE, completed_at = $completed_at, updated_at = $now WHERE status = $expected AND updated_at = $expected_updated_at AND (lease_expires_at = NONE OR lease_expires_at <= $now) RETURN AFTER); IF $updated != NONE { CREATE outbox_event CONTENT $event RETURN NONE; }; RETURN $updated; COMMIT TRANSACTION;",
            )
            .bind(("task", task.task_id.record_id()))
            .bind(("status", status))
            .bind(("request", envelope.into_open_object()?))
            .bind(("error", failure.as_ref().map(failure_to_open_object)))
            .bind(("completed_at", terminal.then_some(now)))
            .bind(("now", now))
            .bind(("expected", task.status))
            .bind(("expected_updated_at", task.updated_at))
            .bind(("event", event))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(3)?;
        let snapshot = updated
            .map(record_to_snapshot)
            .transpose()?
            .ok_or_else(|| TaskError::Conflict(task.task_id.to_string()))?;
        self.note_change();
        Ok(snapshot)
    }
}
