//! Explicit public reconnection after an owner-proved unfinished Task replacement.
use super::*;
use rmcp::model::GetTaskParams;

/// The exact Task has delivered Working and its current official GET agrees.
pub struct WorkingCheckpoint<'a> {
    id: &'a CanonicalTaskId,
    created: &'a Task,
    current: &'a DetailedTask,
    client: &'a SmokeMcpClient,
    end: tokio::time::Instant,
}
impl WorkingCheckpoint<'_> {
    pub fn task_id(&self) -> &CanonicalTaskId {
        self.id
    }
    pub fn created_task(&self) -> &Task {
        self.created
    }
    pub fn current_working(&self) -> &DetailedTask {
        self.current
    }
    pub fn client(&self) -> &SmokeMcpClient {
        self.client
    }
    pub fn deadline(&self) -> tokio::time::Instant {
        self.end
    }
}

/// The owner proves a real process replacement within the original Task deadline.
pub trait WorkingTaskRecovery {
    fn checkpoint<'a>(
        &'a mut self,
        context: WorkingCheckpoint<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Persist the verified replacement observation before another cancellable wait.
    fn replacement_working(
        &mut self,
        id: &CanonicalTaskId,
        created: &Task,
        current: &DetailedTask,
    ) -> Result<()>;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveredTaskResult {
    pub task_id: CanonicalTaskId,
    pub statuses: Vec<TaskStatus>,
    pub result: CallToolResult,
    pub created_task: Task,
    pub replacement_working: DetailedTask,
}

#[derive(Default)]
struct RecoveryHandles {
    generations: Vec<Handles>,
    task_cleanup: Option<owner::CleanupRegistration>,
}
impl RecoveryHandles {
    async fn close(&mut self, end: tokio::time::Instant) -> Result<()> {
        let mut failed = false;
        for generation in &mut self.generations {
            if close_generation(generation, end).await.is_err() {
                failed = true;
            }
        }
        ensure!(!failed, "recovery SDK cleanup failed");
        Ok(())
    }
}
async fn close_generation(handles: &mut Handles, cap: tokio::time::Instant) -> Result<()> {
    let end = *handles.end.get_or_insert(cap);
    let end = end.min(cap);
    handles.end = Some(end);
    if handles.listener_close.is_none()
        && let Some(mut listener) = handles.subscription.take()
    {
        handles.listener_close = Some(Box::pin(async move {
            listener
                .cancel()
                .await
                .map_err(|_| anyhow!("subscription close failed"))
        }));
    }
    let listener = finish_original(&mut handles.listener_close, end, &mut handles.failed).await;
    if handles.client_close.is_none()
        && let Some(client) = handles.client.take()
    {
        handles.client_close = Some(Box::pin(async move {
            client
                .cancel()
                .await
                .map_err(|_| anyhow!("connection close failed"))
        }));
    }
    let client = finish_original(&mut handles.client_close, end, &mut handles.failed).await;
    listener?;
    client?;
    ensure!(!handles.failed, "previous recovery SDK close failed");
    Ok(())
}
async fn finish_original(
    slot: &mut Option<Closing>,
    end: tokio::time::Instant,
    failed: &mut bool,
) -> Result<()> {
    if slot.is_none() {
        return Ok(());
    }
    if tokio::time::Instant::now() >= end {
        *failed = true;
        bail!("recovery SDK cleanup deadline; original close remains retained");
    }
    finish(slot, end, failed).await
}
fn working(created: &Task, current: &DetailedTask, id: &CanonicalTaskId) -> Result<()> {
    task_identity(&current.task, created, id)?;
    ensure!(
        matches!(current.payload, TaskPayload::Working),
        "recovery requires unfinished Working Task"
    );
    Ok(())
}
async fn next_task(
    listener: &mut Subscription,
    created: &Task,
    id: &CanonicalTaskId,
) -> Result<DetailedTask> {
    let notification = safe("Task notification", listener.next().await)?
        .context("Task subscription ended before required delivery")?;
    notification_identity(&notification, listener.id())?;
    let ServerNotification::TaskStatusNotification(update) = notification else {
        bail!("unexpected notification in exact Task subscription");
    };
    task_identity(&update.params.task.task, created, id)?;
    Ok(update.params.task)
}

impl FinalTaskSmokeClient {
    /// Create once, prove unfinished replacement, then reconnect once without dispatch replay.
    pub async fn run_tool_delivered_recovered(
        &self,
        name: &str,
        arguments: Value,
        timeout: Duration,
        mut on_created: impl FnMut(&CanonicalTaskId, &Task) -> Result<()>,
        recovery: &mut impl WorkingTaskRecovery,
    ) -> Result<RecoveredTaskResult> {
        ensure!(
            timeout > Duration::from_secs(2),
            "recovery timeout must include two seconds for SDK cleanup"
        );
        let end = (tokio::time::Instant::now() + timeout).min(tokio::time::Instant::from_std(
            owner::active()
                .context("recovery requires an active owner")?
                .deadline,
        ));
        ensure!(
            end.saturating_duration_since(tokio::time::Instant::now()) > Duration::from_secs(2),
            "original owner deadline has no recovery cleanup reserve"
        );
        let work_end = end - Duration::from_secs(2);
        let retained = Arc::new(Mutex::new(RecoveryHandles::default()));
        let captured = retained.clone();
        let registration = owner::register_cleanup(
            CleanupKind::Remote,
            "recovered MCP SDK generations",
            &uuid::Uuid::now_v7().to_string(),
            move || async move {
                let cap = tokio::time::Instant::from_std(owner::cleanup_deadline()?).min(end);
                captured.lock().await.close(cap).await
            },
        )?;
        let result = tokio::time::timeout_at(
            work_end,
            Box::pin(async {
                let mut state = retained.lock().await;
                let RecoveryHandles {
                    generations,
                    task_cleanup,
                } = &mut *state;
                generations.push(Handles::default());
                let original = generations.last_mut().unwrap();
                original.client = Some(safe("MCP discovery", self.connect().await)?);
                let client = original.client.as_ref().unwrap();
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
                *task_cleanup = Some(
                    client
                        .take_task_cleanup(&id)
                        .context("missing original Task cleanup registration")?,
                );
                on_created(&id, &created)?;
                let filter = SubscriptionFilter::builder()
                    .task_ids([id.to_string()])
                    .build();
                original.subscription =
                    Some(safe("Task listen", client.listen(filter.clone()).await)?);
                let listener = original.subscription.as_mut().unwrap();
                acknowledged(listener.acknowledged(), &filter)?;
                let delivered = next_task(listener, &created, &id).await?;
                working(&created, &delivered, &id)?;
                let current = safe(
                    "tasks/get",
                    client.get_task(GetTaskParams::new(id.as_str())).await,
                )?;
                working(&created, &current.task, &id)?;
                recovery
                    .checkpoint(WorkingCheckpoint {
                        id: &id,
                        created: &created,
                        current: &current.task,
                        client,
                        end: work_end,
                    })
                    .await?;
                // Cancellation does not discard either consuming close future or its first cap.
                close_generation(original, end).await?;
                generations.push(Handles::default());
                let replacement = generations.last_mut().unwrap();
                replacement.client = Some(safe("replacement MCP discovery", self.connect().await)?);
                let client = replacement.client.as_ref().unwrap();
                let current = safe(
                    "replacement tasks/get",
                    client.get_task(GetTaskParams::new(id.as_str())).await,
                )?;
                working(&created, &current.task, &id)?;
                let replacement_working = current.task;
                recovery.replacement_working(&id, &created, &replacement_working)?;
                replacement.subscription = Some(safe(
                    "replacement Task listen",
                    client.listen(filter.clone()).await,
                )?);
                let listener = replacement.subscription.as_mut().unwrap();
                acknowledged(listener.acknowledged(), &filter)?;
                let mut statuses = vec![TaskStatus::Working];
                let delivered = loop {
                    ensure!(
                        statuses.len() < 128,
                        "Task delivery exceeded 128 notifications"
                    );
                    let task = next_task(listener, &created, &id).await?;
                    statuses.push(task.status());
                    match task.status() {
                        TaskStatus::Working => {}
                        TaskStatus::Completed => break task,
                        _ => bail!("Task delivered an unsuccessful terminal or input status"),
                    }
                };
                let current = safe(
                    "completed tasks/get",
                    client.get_task(GetTaskParams::new(id.as_str())).await,
                )?;
                let result = completed(&created, &delivered, &current.task, &id)?;
                task_cleanup
                    .as_ref()
                    .context("original Task cleanup registration unavailable")?
                    .settled()?;
                *task_cleanup = None;
                Ok(RecoveredTaskResult {
                    task_id: id,
                    statuses,
                    result,
                    created_task: created,
                    replacement_working,
                })
            }),
        )
        .await
        .context("recovered Task original operation deadline")
        .and_then(|r| r);
        let closed = retained.lock().await.close(end).await;
        if closed.is_ok() {
            registration.settled()?;
        }
        let result = result?;
        closed?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
