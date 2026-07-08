use std::collections::HashMap;

use anyhow::Context;
use chrono::{DateTime, Utc};
use rmcp::{
    RoleServer,
    model::{
        JsonObject, Meta, Notification, ProgressNotificationParam, ProgressToken,
        ServerNotification, Task, TaskStatus, TaskStatusNotificationParam,
    },
    service::Peer,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::{sync::RwLock, task::JoinHandle};

use crate::gateway::{GatewayTaskId, ResourceUri};

fn is_terminal(status: &TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
    )
}

pub const GATEWAY_TASK_RESOURCE_TEMPLATE: &str = "veoveo://task/{task_id}";

pub fn gateway_task_resource_uri(task_id: &GatewayTaskId) -> ResourceUri {
    ResourceUri::new(format!("veoveo://task/{task_id}"))
        .expect("gateway task resource URI is valid")
}

pub fn parse_gateway_task_resource_uri(uri: &str) -> Option<&str> {
    uri.strip_prefix("veoveo://task/")
        .filter(|task_id| !task_id.is_empty() && !task_id.contains('/'))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GatewayTaskStatusKind {
    Working,
    InputRequired,
    Completed,
    Failed,
    Cancelled,
}

impl TryFrom<&TaskStatus> for GatewayTaskStatusKind {
    type Error = anyhow::Error;

    fn try_from(status: &TaskStatus) -> Result<Self, Self::Error> {
        Ok(match status {
            TaskStatus::Working => Self::Working,
            TaskStatus::InputRequired => Self::InputRequired,
            TaskStatus::Completed => Self::Completed,
            TaskStatus::Failed => Self::Failed,
            TaskStatus::Cancelled => Self::Cancelled,
            _ => anyhow::bail!("unsupported MCP task status: {status:?}"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayTaskStatus {
    pub task_id: GatewayTaskId,
    pub status: GatewayTaskStatusKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_after_ms: Option<u64>,
    pub status_resource: ResourceUri,
    pub result_available: bool,
}

impl GatewayTaskStatus {
    pub fn from_task(task: &Task) -> anyhow::Result<Self> {
        let task_id =
            GatewayTaskId::new(task.task_id.clone()).context("invalid gateway task id")?;
        let created_at = DateTime::parse_from_rfc3339(&task.created_at)
            .context("invalid gateway task created_at timestamp")?
            .with_timezone(&Utc);
        let last_updated_at = DateTime::parse_from_rfc3339(&task.last_updated_at)
            .context("invalid gateway task last_updated_at timestamp")?
            .with_timezone(&Utc);
        Ok(Self {
            status: GatewayTaskStatusKind::try_from(&task.status)?,
            status_message: task.status_message.clone(),
            created_at,
            last_updated_at,
            ttl: task.ttl,
            poll_after_ms: task.poll_interval,
            status_resource: gateway_task_resource_uri(&task_id),
            result_available: is_terminal(&task.status),
            task_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayTaskStatusDocument {
    pub task: GatewayTaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
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

pub const RELATED_TASK_META_KEY: &str = "io.modelcontextprotocol/related-task";

pub fn related_task_meta(task_id: impl Into<String>) -> Meta {
    let mut meta = JsonObject::new();
    meta.insert(
        RELATED_TASK_META_KEY.to_string(),
        serde_json::json!({ "taskId": task_id.into() }),
    );
    Meta(meta)
}

pub fn set_related_task_meta(meta: &mut Option<Meta>, task_id: impl Into<String>) {
    let task_id = task_id.into();
    let mut value = meta.take().unwrap_or_default();
    value.0.insert(
        RELATED_TASK_META_KEY.to_string(),
        serde_json::json!({ "taskId": task_id }),
    );
    *meta = Some(value);
}

pub struct TaskStore {
    tasks: RwLock<HashMap<String, TaskEntry>>,
    /// Bumped on every state mutation so `await_payload_state` long-polls
    /// wake immediately instead of at their fallback tick.
    changed: tokio::sync::watch::Sender<u64>,
}

impl Default for TaskStore {
    fn default() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
            changed: tokio::sync::watch::channel(0).0,
        }
    }
}

impl TaskStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn note_change(&self) {
        self.changed.send_modify(|version| *version += 1);
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
        self.note_change();
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
        let task = entry.task.clone();
        drop(tasks);
        self.note_change();
        Some(task)
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

    /// The result-retrieval state for `tasks/result`, blocking while the task
    /// is non-terminal: MCP 2025-11-25 requires the receiver to hold the
    /// response until the task reaches a terminal status. Unknown task ids
    /// return immediately. Store mutations wake the wait; a fallback tick
    /// guards against missed signals.
    pub async fn await_payload_state(&self, task_id: &str) -> TaskPayloadState {
        let mut changed = self.changed.subscribe();
        loop {
            changed.mark_unchanged();
            let state = self.payload_state_now(task_id).await;
            if !matches!(state, TaskPayloadState::Running) {
                return state;
            }
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), changed.changed())
                .await;
        }
    }

    async fn payload_state_now(&self, task_id: &str) -> TaskPayloadState {
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
        let task = entry.task.clone();
        drop(tasks);
        self.note_change();
        Some(task)
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

    #[test]
    fn related_task_meta_uses_mcp_key_and_task_id() {
        let mut meta = None;
        set_related_task_meta(&mut meta, "task-1");

        let meta = meta.expect("related task meta should be set");
        assert_eq!(
            meta.0
                .get(RELATED_TASK_META_KEY)
                .and_then(|value| value.get("taskId"))
                .and_then(Value::as_str),
            Some("task-1")
        );
    }

    #[test]
    fn gateway_task_status_uses_typed_task_resource_uri() {
        let task = task("gateway-task-1", TaskStatus::Working, Utc::now())
            .with_status_message("accepted")
            .with_poll_interval(5000);

        let status = GatewayTaskStatus::from_task(&task).unwrap();

        assert_eq!(status.task_id.as_str(), "gateway-task-1");
        assert_eq!(status.status, GatewayTaskStatusKind::Working);
        assert_eq!(status.status_message.as_deref(), Some("accepted"));
        assert_eq!(status.poll_after_ms, Some(5000));
        assert_eq!(
            status.status_resource.as_str(),
            "veoveo://task/gateway-task-1"
        );
        assert_eq!(
            parse_gateway_task_resource_uri(status.status_resource.as_str()),
            Some("gateway-task-1")
        );
        assert!(!status.result_available);
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
            store.await_payload_state("cancelled").await,
            TaskPayloadState::Cancelled
        );
    }
}
