//! Recovery preserves each declared completion profile.
use crate::{
    TaskRuntime,
    runtime::recovery_result,
    types::{
        RecoveryClass, RecoveryReport, TaskError, TaskFailure, TaskSnapshot, TaskTransition,
        failure_to_open_object, record_to_snapshot,
    },
};
use chrono::Utc;
use surrealdb::types::SurrealValue;
use veoveo_platform_store::TaskRequestRecord;
use veoveo_platform_store::task_record_id;
use veoveo_platform_store::{TaskRecord, TaskStatus as StoreTaskStatus};
impl TaskRuntime {
    pub async fn recover(&self) -> Result<RecoveryReport, TaskError> {
        self.check_required_contributions()?;
        let mut report = RecoveryReport::default();
        let tasks = self.list().await?;
        for task in tasks {
            self.check_contribution(&task.task_type)?;
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
        let envelope = TaskRequestRecord {
            input: task.request.clone(),
            status_message: Some(message.to_owned()),
            ttl_ms: task.ttl_ms,
            poll_interval_ms: task.poll_interval_ms,
        };
        self.check_contribution(&task.task_type)?;
        let terminal = status == StoreTaskStatus::Failed;
        let contribution = match failure.as_ref() {
            Some(failure) if terminal => {
                self.settlement_contribution(task, crate::TaskSettlement::Failed { failure }, now)?
            }
            _ => crate::TaskContribution::none(),
        };
        let contribution_sql = contribution.sql(false)?;
        let query = self
            .platform_store()
            .client()
            .query(if contribution_sql.is_empty() {
                include_str!("../queries/recovery_transition.surql")
            } else {
                include_str!("../queries/recovery_transition_contribution.surql")
            })
            .bind(("task", task_record_id(task.task_id)))
            .bind(("status", status))
            .bind(("request", envelope.into_value()))
            .bind(("error", failure.as_ref().map(failure_to_open_object)))
            .bind(("completed_at", terminal.then_some(now)))
            .bind(("now", now))
            .bind(("expected", task.status))
            .bind(("expected_updated_at", task.updated_at))
            .bind(("expected_request", TaskRequestRecord::from(task)))
            .bind((
                "expected_owner_context",
                veoveo_platform_store::TaskOwnerRecord::try_from(&task.owner)?,
            ));
        let mut response = contribution
            .bind(query, task.task_id, &task.task_type, task.created_at)
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
