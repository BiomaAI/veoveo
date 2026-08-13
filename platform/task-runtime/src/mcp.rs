//! Projection from durable Platform Store tasks to the official MCP Tasks model.

use rmcp::model::{
    DetailedTask, ErrorData, InputRequest, JsonObject, Task, TaskPayload,
    TaskStatus as McpTaskStatus,
};
use serde_json::{Value, json};

use crate::{TaskError, TaskRuntime, TaskSnapshot, TaskStatus};

/// Projects durable metadata into the seed returned by `CreateTaskResult`.
pub fn task_seed(snapshot: &TaskSnapshot) -> Task {
    let mut task = Task::new(
        snapshot.task_id.to_string(),
        task_status(snapshot.status),
        snapshot.created_at.to_rfc3339(),
        snapshot.updated_at.to_rfc3339(),
    );
    task.status_message = snapshot.status_message.clone();
    task.ttl_ms = snapshot.ttl_ms;
    task.poll_interval_ms = snapshot.poll_interval_ms;
    task
}

/// Projects the current durable state into the status-specific official task union.
pub async fn project_snapshot(
    runtime: &TaskRuntime,
    snapshot: TaskSnapshot,
) -> Result<DetailedTask, TaskError> {
    let task = task_seed(&snapshot);
    let payload = match snapshot.status {
        TaskStatus::Queued | TaskStatus::Running | TaskStatus::CancelRequested => {
            TaskPayload::Working
        }
        TaskStatus::Waiting => {
            let requests = runtime
                .outstanding_inputs(&snapshot.task_id.to_string())
                .await?;
            if requests.is_empty() {
                TaskPayload::Working
            } else {
                let input_requests = requests
                    .into_iter()
                    .map(|(key, request)| {
                        let envelope = json!({
                            "method": request.method,
                            "params": request.params,
                        });
                        let request = serde_json::from_value::<InputRequest>(envelope).map_err(
                            |error| {
                                TaskError::InvalidRecord(format!(
                                    "task input request is not a supported MCP input request: {error}"
                                ))
                            },
                        )?;
                        Ok((key, request))
                    })
                    .collect::<Result<_, TaskError>>()?;
                TaskPayload::InputRequired { input_requests }
            }
        }
        TaskStatus::Succeeded => {
            let result = match snapshot.result {
                Some(Value::Object(result)) => result,
                Some(value) => JsonObject::from_iter([("value".to_owned(), value)]),
                None => {
                    return Err(TaskError::InvalidRecord(
                        "completed task has no durable result".to_owned(),
                    ));
                }
            };
            TaskPayload::Completed { result }
        }
        TaskStatus::Failed => {
            let failure = snapshot.error.ok_or_else(|| {
                TaskError::InvalidRecord("failed task has no durable error".to_owned())
            })?;
            let error = ErrorData::internal_error(
                failure.message,
                Some(json!({
                    "taskCode": failure.code,
                    "details": failure.details,
                })),
            );
            let error = serde_json::to_value(error)
                .map_err(|error| TaskError::InvalidRecord(error.to_string()))?
                .as_object()
                .cloned()
                .expect("MCP error serializes as an object");
            TaskPayload::Failed { error }
        }
        TaskStatus::Cancelled => TaskPayload::Cancelled,
    };
    Ok(DetailedTask::new(task, payload))
}

fn task_status(status: TaskStatus) -> McpTaskStatus {
    match status {
        TaskStatus::Queued | TaskStatus::Running | TaskStatus::CancelRequested => {
            McpTaskStatus::Working
        }
        TaskStatus::Waiting => McpTaskStatus::InputRequired,
        TaskStatus::Succeeded => McpTaskStatus::Completed,
        TaskStatus::Failed => McpTaskStatus::Failed,
        TaskStatus::Cancelled => McpTaskStatus::Cancelled,
    }
}
