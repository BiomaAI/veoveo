//! Recovery preserves each declared completion profile.
use crate::{
    TaskRuntime,
    runtime::recovery_result,
    types::{
        RecoveryClass, RecoveryReport, RequestEnvelope, TaskError, TaskFailure, TaskSnapshot,
        TaskTransition, failure_to_open_object, record_to_snapshot,
    },
};
use chrono::Utc;
use veoveo_platform_store::task_record_id;
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

        let mut response = self
            .platform_store()
            .client()
            .query(
                "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = $status, request = $request, error = $error, lease_owner = NONE, lease_expires_at = NONE, completed_at = $completed_at, updated_at = $now WHERE status = $expected AND updated_at = $expected_updated_at AND (lease_expires_at = NONE OR lease_expires_at <= $now) RETURN AFTER); RETURN $updated; COMMIT TRANSACTION;",
            )
            .bind(("task", task_record_id(task.task_id)))
            .bind(("status", status))
            .bind(("request", envelope.into_open_object()?))
            .bind(("error", failure.as_ref().map(failure_to_open_object)))
            .bind(("completed_at", terminal.then_some(now)))
            .bind(("now", now))
            .bind(("expected", task.status))
            .bind(("expected_updated_at", task.updated_at))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(2)?;
        let snapshot = updated
            .map(record_to_snapshot)
            .transpose()?
            .ok_or_else(|| TaskError::Conflict(task.task_id.to_string()))?;
        self.note_change();
        Ok(snapshot)
    }
}
