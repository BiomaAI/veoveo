//! Input answers commit with the current Task status and caller selection.
use std::collections::BTreeMap;

use chrono::Utc;
use surrealdb::types::RecordId;
use veoveo_platform_store::{OpenObject, TaskInputRecord, TaskStatus, task_record_id};
use veoveo_types::TaskId;

use super::{
    MAX_TRANSACTION_ATTEMPTS, OwnerTaskQuery, TaskRuntime, is_retryable_transaction_failure,
    owner_reads, task_input_record, transaction_retry_backoff, validate_input_key,
};
use crate::{TaskError, TaskInputSubmission};

impl OwnerTaskQuery {
    /// Answer outstanding inputs under this query's current owner, clearance,
    /// operation and optional Work Context selection. Each answer commits separately.
    pub async fn submit_input_responses(
        &self,
        task: TaskId,
        responses: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    ) -> Result<TaskInputSubmission, TaskError> {
        self.runtime
            .submit_selected_input_responses(task, responses, Some(self))
            .await
    }
}

impl TaskRuntime {
    /// Trusted worker input submission. Caller-facing adapters use `OwnerTaskQuery`.
    pub async fn submit_input_responses(
        &self,
        task: TaskId,
        responses: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    ) -> Result<TaskInputSubmission, TaskError> {
        self.submit_selected_input_responses(task, responses, None)
            .await
    }

    async fn submit_selected_input_responses(
        &self,
        task: TaskId,
        responses: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
        selection: Option<&OwnerTaskQuery>,
    ) -> Result<TaskInputSubmission, TaskError> {
        let current = self.selected_snapshot(task, selection).await?;
        if current.is_terminal() || current.status == TaskStatus::CancelRequested {
            return Err(TaskError::InvalidTransition {
                from: current.status,
                to: TaskStatus::Running,
            });
        }
        // Both fragments are owned by the runtime. Caller values enter as bindings.
        let admission = selection
            .map(|query| {
                format!(
                    "AND {} {}",
                    owner_reads::VISIBLE_TASK,
                    query.selection_predicate()
                )
            })
            .unwrap_or_default();
        let sql =
            include_str!("input_responses.surql").replace("/* caller selection */", &admission);
        let mut submission = TaskInputSubmission::default();
        for (key, response_value) in responses {
            validate_input_key(&key)?;
            let mut attempt = 0;
            let accepted = loop {
                let query = self
                    .store
                    .client()
                    .query(sql.clone())
                    .bind(("input", task_input_record(task, &key)))
                    .bind(("key", key.clone()))
                    .bind(("response", OpenObject::new(response_value.clone())))
                    .bind(("now", Utc::now()))
                    .bind(("task", task_record_id(task)))
                    .bind(("server", RecordId::new("mcp_server", self.server.clone())));
                let query = match selection {
                    Some(selection) => selection.bind(query)?,
                    None => query,
                };
                let result = query.await.and_then(|mut response| {
                    match veoveo_platform_store::primary_transaction_error(response.take_errors()) {
                        Some(error) => Err(error),
                        None => Ok(response),
                    }
                });
                match result {
                    Ok(mut response) => break response.take::<Option<TaskInputRecord>>(3)?,
                    Err(error)
                        if is_retryable_transaction_failure(&error)
                            && attempt + 1 < MAX_TRANSACTION_ATTEMPTS =>
                    {
                        transaction_retry_backoff(attempt).await;
                        attempt += 1;
                    }
                    Err(error) => return Err(TaskError::Database(error)),
                }
            };
            if accepted.is_some() {
                submission.accepted += 1;
                self.note_change();
            } else {
                submission.ignored += 1;
            }
        }
        Ok(submission)
    }
}
