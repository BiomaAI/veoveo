//! Official Tasks delivery and initial-current resource snapshot assertions.
use super::*;
use crate::lifecycle::owner::{self, CleanupKind};
use rmcp::{
    model::{DetailedTask, GetMeta, ServerNotification, SubscriptionFilter, Task, TaskStatus},
    service::Subscription,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{future::Future, pin::Pin, sync::Arc};
use tokio::sync::Mutex;
use veoveo_types::{CanonicalTaskId, ResourceUri};

mod recovery;
pub use recovery::{RecoveredTaskResult, WorkingCheckpoint, WorkingTaskRecovery};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveredTaskResult {
    pub task_id: CanonicalTaskId,
    pub statuses: Vec<TaskStatus>,
    pub result: CallToolResult,
}
/// An acknowledged resource subscription delivered its initial current snapshot.
/// This does not establish a mutation, restart or cross-replica transition.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSnapshotDelivery {
    pub uri: ResourceUri,
}

type Closing = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
#[derive(Default)]
struct Handles {
    client: Option<SmokeMcpClient>,
    subscription: Option<Subscription>,
    listener_close: Option<Closing>,
    client_close: Option<Closing>,
    failed: bool,
    end: Option<tokio::time::Instant>,
}
impl Handles {
    async fn close(&mut self) -> Result<()> {
        let end = *self.end.get_or_insert(
            tokio::time::Instant::from_std(owner::cleanup_deadline()?)
                .min(tokio::time::Instant::now() + Duration::from_secs(10)),
        );
        if self.listener_close.is_none()
            && let Some(mut subscription) = self.subscription.take()
        {
            self.listener_close = Some(Box::pin(async move {
                subscription
                    .cancel()
                    .await
                    .map_err(|_| anyhow!("subscription close failed"))
            }));
        }
        let listener = finish(&mut self.listener_close, end, &mut self.failed).await;
        if self.client_close.is_none()
            && let Some(client) = self.client.take()
        {
            self.client_close = Some(Box::pin(async move {
                client
                    .cancel()
                    .await
                    .map_err(|_| anyhow!("connection close failed"))
            }));
        }
        let client = finish(&mut self.client_close, end, &mut self.failed).await;
        listener?;
        client?;
        ensure!(!self.failed, "previous SDK close failed");
        Ok(())
    }
}
async fn finish(
    slot: &mut Option<Closing>,
    end: tokio::time::Instant,
    failed: &mut bool,
) -> Result<()> {
    let Some(close) = slot.as_mut() else {
        return Ok(());
    };
    match tokio::time::timeout_at(end, close).await {
        Ok(result) => {
            *slot = None;
            if result.is_err() {
                *failed = true;
            }
            result
        }
        Err(_) => {
            *failed = true;
            bail!("SDK cleanup deadline; original close remains retained")
        }
    }
}
fn retained() -> Result<(Arc<Mutex<Handles>>, owner::CleanupRegistration)> {
    let handles = Arc::new(Mutex::new(Handles::default()));
    let captured = handles.clone();
    let registration = owner::register_cleanup(
        CleanupKind::Remote,
        "delivered MCP SDK handles",
        &uuid::Uuid::now_v7().to_string(),
        move || async move { captured.lock().await.close().await },
    )?;
    Ok((handles, registration))
}
fn safe<T, E: std::fmt::Display>(method: &str, value: std::result::Result<T, E>) -> Result<T> {
    value.map_err(|error| {
        let digest = hex::encode(Sha256::digest(error.to_string().as_bytes()));
        anyhow!("{method} failed (diagnostic sha256:{digest})")
    })
}
fn acknowledged(actual: &SubscriptionFilter, expected: &SubscriptionFilter) -> Result<()> {
    ensure!(
        actual == expected,
        "acknowledged subscription filter differs"
    );
    Ok(())
}
fn notification_identity(
    notification: &ServerNotification,
    subscription: &rmcp::model::RequestId,
) -> Result<()> {
    ensure!(
        notification.get_meta().subscription_id() == Some(subscription.clone()),
        "notification subscription identity differs"
    );
    Ok(())
}
fn task_identity(task: &Task, created: &Task, id: &CanonicalTaskId) -> Result<()> {
    ensure!(
        CanonicalTaskId::parse(&task.task_id).ok().as_ref() == Some(id)
            && task.created_at == created.created_at,
        "Task identity or creation time differs"
    );
    Ok(())
}
fn completed(
    created: &Task,
    delivered: &DetailedTask,
    current: &DetailedTask,
    id: &CanonicalTaskId,
) -> Result<CallToolResult> {
    task_identity(&delivered.task, created, id)?;
    task_identity(&current.task, created, id)?;
    ensure!(
        delivered.payload == current.payload,
        "delivered and current completed Task disagree"
    );
    let TaskPayload::Completed { result } = &delivered.payload else {
        bail!("Task did not deliver Completed");
    };
    let result: CallToolResult = safe(
        "completed tool result admission",
        serde_json::from_value(Value::Object(result.clone())),
    )?;
    ensure!(
        result.is_error != Some(true),
        "completed Task returned a tool error"
    );
    Ok(result)
}
fn resource_delivery(
    notification: &ServerNotification,
    subscription: &rmcp::model::RequestId,
    uri: &ResourceUri,
) -> Result<()> {
    notification_identity(notification, subscription)?;
    let ServerNotification::ResourceUpdatedNotification(update) = notification else {
        bail!("unexpected notification in exact resource subscription");
    };
    ensure!(
        safe(
            "resource notification identity admission",
            ResourceUri::new(update.params.uri.clone())
        )? == *uri,
        "resource notification identity differs"
    );
    Ok(())
}
#[cfg(test)]
mod tests;

impl FinalTaskSmokeClient {
    pub(super) async fn read_resource_owned<T: DeserializeOwned>(
        &self,
        uri: &ResourceUri,
    ) -> Result<T> {
        let (handles, registration) = retained()?;
        let result = tokio::time::timeout(Duration::from_secs(30), async {
            let mut handles = handles.lock().await;
            handles.client = Some(safe("MCP discovery", self.connect().await)?);
            let value = safe(
                "resources/read",
                read_mcp_resource_json(handles.client.as_ref().unwrap(), uri.as_str()).await,
            )?;
            safe("resource contract admission", serde_json::from_value(value))
        })
        .await
        .context("resource read operation deadline")
        .and_then(|r| r);
        let closed = handles.lock().await.close().await;
        if closed.is_ok() {
            registration.settled()?;
        }
        let result = result?;
        closed?;
        Ok(result)
    }

    /// Requires an active lifecycle owner; its registry retains the actual SDK close futures.
    pub async fn run_tool_delivered(
        &self,
        name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<DeliveredTaskResult> {
        self.run_tool_delivered_observed(name, arguments, timeout, |_| Ok(()))
            .await
    }

    /// Persist the admitted creation identity before any delivery wait can be cancelled.
    pub async fn run_tool_delivered_observed(
        &self,
        name: &str,
        arguments: Value,
        timeout: Duration,
        mut on_created: impl FnMut(&CanonicalTaskId) -> Result<()>,
    ) -> Result<DeliveredTaskResult> {
        let (handles, registration) = retained()?;
        let result = tokio::time::timeout(timeout, async {
            // Acquire ownership before connection/listen so successful handles are retained
            // synchronously before the next cancellation point.
            let mut handles = handles.lock().await;
            handles.client = Some(safe("MCP discovery", self.connect().await)?);
            let client = handles.client.as_ref().unwrap();
            ensure!(
                client
                    .peer_info()
                    .is_some_and(|info| info.capabilities.supports_tasks()),
                "server does not advertise official MCP Tasks"
            );
            let created = safe(
                "tools/call Task creation",
                call_tool_as_task(client, name, arguments).await,
            )?;
            let id = safe(
                "acknowledged Task ID admission",
                CanonicalTaskId::parse(&created.task_id),
            )?;
            on_created(&id)?;
            let filter = SubscriptionFilter::builder()
                .task_ids([id.to_string()])
                .build();
            let listener = safe("Task listen", client.listen(filter.clone()).await)?;
            handles.subscription = Some(listener);
            let handles = &mut *handles;
            let listener = handles.subscription.as_mut().unwrap();
            acknowledged(listener.acknowledged(), &filter)?;
            let mut statuses = Vec::new();
            let delivered = loop {
                ensure!(
                    statuses.len() < 128,
                    "Task delivery exceeded 128 notifications"
                );
                let notification = safe("Task notification", listener.next().await)?
                    .context("Task subscription ended before Completed delivery")?;
                notification_identity(&notification, listener.id())?;
                let ServerNotification::TaskStatusNotification(update) = notification else {
                    bail!("unexpected notification in exact Task subscription");
                };
                let task = update.params.task;
                task_identity(&task.task, &created, &id)?;
                statuses.push(task.status());
                match task.status() {
                    TaskStatus::Working => {}
                    TaskStatus::Completed => break task,
                    _ => bail!("Task delivered an unsuccessful terminal or input status"),
                }
            };
            let client = handles.client.as_ref().unwrap();
            let current = safe(
                "tasks/get",
                crate::await_task_terminal_with_timeout(
                    client,
                    id.as_str(),
                    Duration::from_secs(10),
                )
                .await,
            )?;
            let result = completed(&created, &delivered, &current, &id)?;
            Ok(DeliveredTaskResult {
                task_id: id,
                statuses,
                result,
            })
        })
        .await
        .context("Task delivery operation deadline")
        .and_then(|r| r);
        let closed = handles.lock().await.close().await;
        if closed.is_ok() {
            registration.settled()?;
        }
        let result = result?;
        closed?;
        Ok(result)
    }

    pub async fn resource_snapshot_delivery(
        &self,
        uri: &ResourceUri,
        timeout: Duration,
    ) -> Result<ResourceSnapshotDelivery> {
        let (handles, registration) = retained()?;
        let result = tokio::time::timeout(timeout, async {
            let mut handles = handles.lock().await;
            handles.client = Some(safe("MCP discovery", self.connect().await)?);
            let filter = SubscriptionFilter::builder()
                .resource_subscriptions([uri.to_string()])
                .build();
            let subscription = safe(
                "resource listen",
                handles
                    .client
                    .as_ref()
                    .unwrap()
                    .listen(filter.clone())
                    .await,
            )?;
            handles.subscription = Some(subscription);
            let listener = handles.subscription.as_mut().unwrap();
            acknowledged(listener.acknowledged(), &filter)?;
            let notification = safe("resource notification", listener.next().await)?
                .context("resource subscription ended before snapshot delivery")?;
            resource_delivery(&notification, listener.id(), uri)?;
            Ok(ResourceSnapshotDelivery { uri: uri.clone() })
        })
        .await
        .context("resource snapshot operation deadline")
        .and_then(|r| r);
        let closed = handles.lock().await.close().await;
        if closed.is_ok() {
            registration.settled()?;
        }
        let result = result?;
        closed?;
        Ok(result)
    }
}
