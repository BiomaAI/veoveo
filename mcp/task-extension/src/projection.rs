use std::collections::BTreeMap;

use serde_json::Value;
use veoveo_task_runtime::{TaskError, TaskRuntime, TaskSnapshot, TaskStatus as StoreTaskStatus};

use crate::{
    DetailedTask, EmbeddedRequest, JsonRpcErrorData, ProtocolTaskId, Task, TaskMetadata, TaskStatus,
};

pub fn task_seed(snapshot: &TaskSnapshot) -> Task {
    Task {
        task_id: ProtocolTaskId::from(snapshot.task_id),
        status: task_status(snapshot.status),
        status_message: snapshot.status_message.clone(),
        created_at: snapshot.created_at,
        last_updated_at: snapshot.updated_at,
        ttl_ms: snapshot.ttl_ms,
        poll_interval_ms: snapshot.poll_interval_ms,
    }
}

pub async fn project_snapshot(
    runtime: &TaskRuntime,
    snapshot: TaskSnapshot,
) -> Result<DetailedTask, TaskError> {
    let metadata = TaskMetadata {
        task_id: ProtocolTaskId::from(snapshot.task_id),
        status_message: snapshot.status_message.clone(),
        created_at: snapshot.created_at,
        last_updated_at: snapshot.updated_at,
        ttl_ms: snapshot.ttl_ms,
        poll_interval_ms: snapshot.poll_interval_ms,
    };
    match snapshot.status {
        StoreTaskStatus::Queued | StoreTaskStatus::Running | StoreTaskStatus::CancelRequested => {
            Ok(DetailedTask::Working { metadata })
        }
        StoreTaskStatus::Waiting => {
            let requests = runtime
                .outstanding_inputs(&snapshot.task_id.to_string())
                .await?;
            if requests.is_empty() {
                Ok(DetailedTask::Working { metadata })
            } else {
                Ok(DetailedTask::InputRequired {
                    metadata,
                    input_requests: requests
                        .into_iter()
                        .map(|(key, request)| {
                            (
                                key,
                                EmbeddedRequest {
                                    method: request.method,
                                    params: request.params,
                                },
                            )
                        })
                        .collect(),
                })
            }
        }
        StoreTaskStatus::Succeeded => {
            let result = match snapshot.result {
                Some(Value::Object(result)) => result.into_iter().collect(),
                Some(value) => BTreeMap::from([("value".to_owned(), value)]),
                None => {
                    return Err(TaskError::InvalidRecord(
                        "completed task has no durable result".to_owned(),
                    ));
                }
            };
            Ok(DetailedTask::Completed { metadata, result })
        }
        StoreTaskStatus::Failed => {
            let failure = snapshot.error.ok_or_else(|| {
                TaskError::InvalidRecord("failed task has no durable error".to_owned())
            })?;
            Ok(DetailedTask::Failed {
                metadata,
                error: JsonRpcErrorData {
                    code: -32_603,
                    message: failure.message,
                    data: Some(serde_json::json!({
                        "taskCode": failure.code,
                        "details": failure.details,
                    })),
                },
            })
        }
        StoreTaskStatus::Cancelled => Ok(DetailedTask::Cancelled { metadata }),
    }
}

fn task_status(status: StoreTaskStatus) -> TaskStatus {
    match status {
        StoreTaskStatus::Queued
        | StoreTaskStatus::Running
        | StoreTaskStatus::Waiting
        | StoreTaskStatus::CancelRequested => TaskStatus::Working,
        StoreTaskStatus::Succeeded => TaskStatus::Completed,
        StoreTaskStatus::Failed => TaskStatus::Failed,
        StoreTaskStatus::Cancelled => TaskStatus::Cancelled,
    }
}
