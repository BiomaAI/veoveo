use std::collections::HashMap;

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

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

struct TaskEntry {
    task: Task,
    payload: Option<Value>,
    error: Option<String>,
    provider_job_id: Option<String>,
    join: Option<JoinHandle<()>>,
}

pub enum TaskPayloadState {
    Completed(Value),
    Failed(String),
    Cancelled,
    Running,
    Unknown,
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

    pub async fn list(&self) -> Vec<Task> {
        self.tasks
            .read()
            .await
            .values()
            .map(|entry| entry.task.clone())
            .collect()
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
        if !matches!(
            entry.task.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            if let Some(join) = entry.join.take() {
                join.abort();
            }
            entry.task.status = TaskStatus::Cancelled;
            entry.task.status_message = Some("cancelled by client".into());
            entry.task.last_updated_at = now_iso();
        }
        Some(entry.task.clone())
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
