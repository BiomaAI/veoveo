use std::collections::HashMap;

use chrono::{DateTime, Utc};
use rmcp::{
    RoleServer,
    model::{
        Notification, ProgressNotificationParam, ProgressToken, ServerNotification, Task,
        TaskStatus, TaskStatusNotificationParam,
    },
    service::Peer,
};
use serde_json::Value;
use tokio::{sync::RwLock, task::JoinHandle};

fn is_terminal(status: &TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
    )
}

pub fn now_utc() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

pub fn now_iso() -> String {
    now_utc().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

struct TaskEntry {
    task: Task,
    payload: Option<Value>,
    error: Option<String>,
    provider_job_id: Option<String>,
    join: Option<JoinHandle<()>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaskPayloadState {
    Completed(Value),
    Failed(String),
    Cancelled,
    Running,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrunedTask {
    pub task_id: String,
    pub provider_job_id: Option<String>,
}

#[derive(Default)]
pub struct TaskStore {
    tasks: RwLock<HashMap<String, TaskEntry>>,
}

impl TaskStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert(&self, task: Task, join: Option<JoinHandle<()>>) {
        self.insert_record(task, None, None, None, join).await;
    }

    pub async fn insert_record(
        &self,
        task: Task,
        payload: Option<Value>,
        error: Option<String>,
        provider_job_id: Option<String>,
        join: Option<JoinHandle<()>>,
    ) {
        self.tasks.write().await.insert(
            task.task_id.clone(),
            TaskEntry {
                task,
                payload,
                error,
                provider_job_id,
                join,
            },
        );
    }

    pub async fn set_provider_job_id(&self, task_id: &str, provider_job_id: impl Into<String>) {
        if let Some(entry) = self.tasks.write().await.get_mut(task_id) {
            entry.provider_job_id = Some(provider_job_id.into());
        }
    }

    pub async fn set_join(&self, task_id: &str, join: JoinHandle<()>) {
        if let Some(entry) = self.tasks.write().await.get_mut(task_id) {
            entry.join = Some(join);
        }
    }

    pub async fn provider_job_id(&self, task_id: &str) -> Option<String> {
        self.tasks
            .read()
            .await
            .get(task_id)
            .and_then(|entry| entry.provider_job_id.clone())
    }

    pub async fn update(
        &self,
        task_id: &str,
        status: TaskStatus,
        message: impl Into<String>,
        payload: Option<Value>,
        error: Option<String>,
    ) -> Option<Task> {
        let mut tasks = self.tasks.write().await;
        let entry = tasks.get_mut(task_id)?;
        if is_terminal(&entry.task.status) {
            return None;
        }
        entry.task.status = status;
        entry.task.status_message = Some(message.into());
        entry.task.last_updated_at = now_iso();
        if payload.is_some() {
            entry.payload = payload;
        }
        if error.is_some() {
            entry.error = error;
        }
        Some(entry.task.clone())
    }

    pub async fn get(&self, task_id: &str) -> Option<Task> {
        self.tasks
            .read()
            .await
            .get(task_id)
            .map(|entry| entry.task.clone())
    }

    pub async fn is_terminal(&self, task_id: &str) -> bool {
        self.tasks
            .read()
            .await
            .get(task_id)
            .is_some_and(|entry| is_terminal(&entry.task.status))
    }

    pub async fn list(&self) -> Vec<Task> {
        let mut tasks: Vec<Task> = self
            .tasks
            .read()
            .await
            .values()
            .map(|entry| entry.task.clone())
            .collect();
        tasks.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.task_id.cmp(&b.task_id))
        });
        tasks
    }

    pub async fn payload_state(&self, task_id: &str) -> TaskPayloadState {
        let tasks = self.tasks.read().await;
        let Some(entry) = tasks.get(task_id) else {
            return TaskPayloadState::Unknown;
        };
        match entry.task.status {
            TaskStatus::Completed => entry
                .payload
                .clone()
                .map(TaskPayloadState::Completed)
                .unwrap_or_else(|| {
                    TaskPayloadState::Failed("completed task lost its payload".to_string())
                }),
            TaskStatus::Failed => TaskPayloadState::Failed(
                entry
                    .error
                    .clone()
                    .unwrap_or_else(|| "task failed".to_string()),
            ),
            TaskStatus::Cancelled => TaskPayloadState::Cancelled,
            _ => TaskPayloadState::Running,
        }
    }

    pub async fn cancel(&self, task_id: &str) -> Option<Task> {
        let mut tasks = self.tasks.write().await;
        let entry = tasks.get_mut(task_id)?;
        if !is_terminal(&entry.task.status) {
            if let Some(join) = entry.join.take() {
                join.abort();
            }
            entry.task.status = TaskStatus::Cancelled;
            entry.task.status_message = Some("cancelled by client".into());
            entry.task.last_updated_at = now_iso();
        }
        Some(entry.task.clone())
    }

    pub async fn prune_terminal_before(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<PrunedTask>, chrono::ParseError> {
        let mut tasks = self.tasks.write().await;
        let mut expired = Vec::new();
        for (task_id, entry) in tasks.iter() {
            if !is_terminal(&entry.task.status) {
                continue;
            }
            let updated_at =
                DateTime::parse_from_rfc3339(&entry.task.last_updated_at)?.with_timezone(&Utc);
            if updated_at < cutoff {
                expired.push(PrunedTask {
                    task_id: task_id.clone(),
                    provider_job_id: entry.provider_job_id.clone(),
                });
            }
        }
        for task in &expired {
            tasks.remove(&task.task_id);
        }
        Ok(expired)
    }
}

pub async fn notify_task_status(peer: &Peer<RoleServer>, task: Task) {
    let _ = peer
        .send_notification(ServerNotification::TaskStatusNotification(
            Notification::new(TaskStatusNotificationParam::new(task)),
        ))
        .await;
}

pub async fn notify_progress(
    peer: &Peer<RoleServer>,
    token: &Option<ProgressToken>,
    progress: f64,
    message: &str,
) {
    if let Some(token) = token {
        let _ = peer
            .notify_progress(
                ProgressNotificationParam::new(token.clone(), progress)
                    .with_total(1.0)
                    .with_message(message),
            )
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(task_id: &str, status: TaskStatus, last_updated_at: DateTime<Utc>) -> Task {
        Task::new(
            task_id.to_string(),
            status,
            last_updated_at.to_rfc3339(),
            last_updated_at.to_rfc3339(),
        )
    }

    #[tokio::test]
    async fn prune_terminal_before_removes_only_expired_terminal_tasks() {
        let store = TaskStore::new();
        let now = Utc::now();
        store
            .insert(task("old-completed", TaskStatus::Completed, now), None)
            .await;
        store
            .insert(task("old-working", TaskStatus::Working, now), None)
            .await;
        store
            .insert(
                task(
                    "fresh-completed",
                    TaskStatus::Completed,
                    now + chrono::TimeDelta::days(2),
                ),
                None,
            )
            .await;

        let pruned = store
            .prune_terminal_before(now + chrono::TimeDelta::days(1))
            .await
            .unwrap();
        assert_eq!(
            pruned,
            vec![PrunedTask {
                task_id: "old-completed".to_string(),
                provider_job_id: None,
            }]
        );

        assert!(store.get("old-completed").await.is_none());
        assert!(store.get("old-working").await.is_some());
        assert!(store.get("fresh-completed").await.is_some());
    }

    #[tokio::test]
    async fn update_does_not_overwrite_terminal_task() {
        let store = TaskStore::new();
        let now = Utc::now();
        store
            .insert(task("cancelled", TaskStatus::Cancelled, now), None)
            .await;

        assert!(
            store
                .update(
                    "cancelled",
                    TaskStatus::Completed,
                    "late provider webhook",
                    Some(serde_json::json!({"artifact": "late"})),
                    None,
                )
                .await
                .is_none()
        );

        let task = store.get("cancelled").await.unwrap();
        assert_eq!(task.status, TaskStatus::Cancelled);
        assert!(store.is_terminal("cancelled").await);
        assert_eq!(
            store.payload_state("cancelled").await,
            TaskPayloadState::Cancelled
        );
    }
}
