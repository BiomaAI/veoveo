use anyhow::Context;
use chrono::{DateTime, Utc};
use rmcp::{
    RoleServer,
    model::{JsonObject, Meta, ProgressNotificationParam, ProgressToken, Task, TaskStatus},
    service::Peer,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

use crate::gateway::{ProviderTaskId, ResourceUri};

const NOTIFICATION_DELIVERY_TIMEOUT: Duration = Duration::from_secs(2);

fn is_terminal(status: &TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
    )
}

pub const GATEWAY_TASK_RESOURCE_TEMPLATE: &str = "veoveo://task/{task_id}";

pub fn gateway_task_resource_uri(task_id: &ProviderTaskId) -> ResourceUri {
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
    pub task_id: ProviderTaskId,
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
            ProviderTaskId::new(task.task_id.clone()).context("invalid provider task id")?;
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

pub async fn notify_progress(
    peer: &Peer<RoleServer>,
    token: &Option<ProgressToken>,
    progress: f64,
    message: &str,
) {
    if let Some(token) = token {
        match tokio::time::timeout(
            NOTIFICATION_DELIVERY_TIMEOUT,
            peer.notify_progress(
                ProgressNotificationParam::new(token.clone(), progress)
                    .with_total(1.0)
                    .with_message(message),
            ),
        )
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(%error, "failed to deliver MCP progress notification");
            }
            Err(_) => {
                tracing::warn!("timed out delivering MCP progress notification");
            }
        }
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
}
