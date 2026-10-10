//! One unfinished read-only analytical query across an admitted process replacement.
use super::*;
use rmcp::model::{CallToolResult, DetailedTask, Task, TaskPayload};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use veoveo_duckdb_mcp::contract::{
    DuckDbColumn, DuckDbQueryUsage, DuckDbTaskUsageUri, DuckDbUsageDetails, DuckDbUsageIndexUri,
    DuckDbUsagePage,
};
use veoveo_types::{ResourceAddress, Sha256Digest, TaskId};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Input {
    request: DuckDbQueryRequest,
    expected: Expected,
    deadline_seconds: u64,
    reserved_work_context: veoveo_types::WorkContextId,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Expected {
    columns: Vec<DuckDbColumn>,
    rows: Vec<Vec<Value>>,
}
impl Input {
    pub fn admit(&self, selected: &DuckDbDatabaseId) -> Result<()> {
        ensure!(
            self.request.database() == selected && self.request.attachments().is_empty(),
            "working query must use only the selected database"
        );
        ensure!(
            matches!(self.request.output(), DuckDbQueryOutputMode::Inline {}),
            "working query requires inline semantic output"
        );
        ensure!(
            (180..=600).contains(&self.deadline_seconds),
            "working query deadline must be 180..600 seconds"
        );
        let timeout = self
            .request
            .timeout_ms()
            .context("working query requires an explicit SQL timeout")?
            .get();
        ensure!(
            (1..=120_000).contains(&timeout),
            "working query timeout exceeds supported 120-second profile"
        );
        // The selected owner opens database and attachments read-only and denies
        // file exchange. Its engine owns SQL semantics; this is not a keyword filter.
        veoveo_duckdb_runtime::validate_single_statement(self.request.sql().as_str())
            .map_err(|_| anyhow!("working query requires one owner-admitted SQL statement"))?;
        ensure!(
            !self.expected.columns.is_empty()
                && self.expected.columns.len() <= 16
                && !self.expected.rows.is_empty()
                && self.expected.rows.len() <= 100,
            "working query requires 1..16 expected columns and 1..100 rows"
        );
        let mut names = BTreeSet::new();
        for column in &self.expected.columns {
            ensure!(
                !column.name.is_empty()
                    && !column.type_name.is_empty()
                    && names.insert(&column.name),
                "expected columns must have distinct names and explicit types"
            );
        }
        ensure!(
            self.expected
                .rows
                .iter()
                .all(|r| r.len() == self.expected.columns.len()),
            "expected row width differs from selected columns"
        );
        let limit = self
            .request
            .row_limit()
            .context("working query requires an explicit row limit")?
            .get();
        ensure!(
            limit >= self.expected.rows.len() as u64 && limit <= 100,
            "working query row limit must cover expected rows within 100"
        );
        Ok(())
    }
    fn require_output(&self, payload: &CallToolResult) -> Result<DuckDbQueryOutput> {
        ensure!(
            payload.is_error != Some(true),
            "working query returned a tool error"
        );
        let output: DuckDbQueryOutput = serde_json::from_value(
            payload
                .structured_content
                .clone()
                .context("query omitted structured output")?,
        )
        .map_err(|_| anyhow!("query output failed owner admission"))?;
        ensure!(
            output.artifact().is_none()
                && !output.truncated()
                && output.row_count() == self.expected.rows.len() as u64,
            "query output count/inline/truncation differs"
        );
        let mut indices = Vec::new();
        for expected in &self.expected.columns {
            let found: Vec<_> = output
                .columns()
                .iter()
                .enumerate()
                .filter(|(_, c)| c.name == expected.name && c.type_name == expected.type_name)
                .map(|(i, _)| i)
                .collect();
            ensure!(
                found.len() == 1,
                "query expected column name/type is missing or duplicated"
            );
            indices.push(found[0]);
        }
        let selected: Vec<Vec<Value>> = output
            .rows()
            .iter()
            .map(|row| indices.iter().map(|i| row[*i].clone()).collect())
            .collect();
        ensure!(
            selected == self.expected.rows,
            "query semantic rows differ from independently expected values"
        );
        Ok(output)
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Identity {
    task_id: CanonicalTaskId,
    created_at: chrono::DateTime<chrono::Utc>,
}
fn identity(created: &Task) -> Result<Identity> {
    Ok(Identity {
        task_id: CanonicalTaskId::parse(&created.task_id)
            .map_err(|_| anyhow!("invalid acknowledged Task ID"))?,
        created_at: chrono::DateTime::parse_from_rfc3339(&created.created_at)
            .map_err(|_| anyhow!("invalid Task creation timestamp"))?
            .with_timezone(&chrono::Utc),
    })
}
fn same(task: &Task, original: &Identity) -> Result<()> {
    let observed = identity(task)?;
    ensure!(
        observed.task_id == original.task_id && observed.created_at == original.created_at,
        "working query Task identity/creation time changed"
    );
    Ok(())
}
fn working(task: &DetailedTask, original: &Identity) -> Result<()> {
    same(&task.task, original)?;
    ensure!(
        task.status() == TaskStatus::Working,
        "unfinished recovery unqualified: original Task is no longer Working"
    );
    Ok(())
}
fn completed(
    delivered: &DetailedTask,
    current: &DetailedTask,
    original: &Identity,
) -> Result<CallToolResult> {
    same(&delivered.task, original)?;
    same(&current.task, original)?;
    ensure!(
        delivered.status() == TaskStatus::Completed
            && current.status() == TaskStatus::Completed
            && delivered.payload == current.payload,
        "delivered/current completed query payload differs"
    );
    let TaskPayload::Completed { result } = &current.payload else {
        bail!("query did not complete");
    };
    serde_json::from_value(Value::Object(result.clone()))
        .map_err(|_| anyhow!("completed query result admission failed"))
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum Event {
    Admitted,
    SelectedProcess,
    DispatchIntent,
    TaskAcknowledged,
    SubscriptionAcknowledged,
    TaskDelivered,
    WorkingDelivered,
    WorkingCurrent,
    RestartIntent,
    RestartObserved,
    ReplacementWorking,
    CompletedDelivered,
    UsageVerified,
    Passed,
    Failed,
    OperationDropped,
    CleanupIntent,
    CleanupClosed,
    CleanupUnqualified,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Observation<'a> {
    schema: Schema,
    event: Event,
    task_id: Option<CanonicalTaskId>,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    status: Option<TaskStatus>,
    subscription_id: Option<rmcp::model::RequestId>,
    selected_process: Option<&'a SelectedDrainIdentity>,
    drain: Option<&'a DrainReceipt>,
    request_sha256: Sha256Digest,
    native_usage: Option<TaskId>,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Schema {
    #[vocabulary(rename = "veoveo.ai/installed-cpu-host-working-query/v1")]
    V1,
}
pub(super) struct Journal {
    file: Mutex<File>,
    identity: Mutex<Option<Identity>>,
    acknowledged: Mutex<Option<CanonicalTaskId>>,
    subscription: Mutex<Option<rmcp::model::RequestId>>,
    selected: Mutex<Option<SelectedDrainIdentity>>,
    request_sha256: Sha256Digest,
    usage: Mutex<Option<TaskId>>,
}
impl Journal {
    fn create(path: &Path, input: &Input) -> Result<Self> {
        ensure!(
            path.is_absolute(),
            "working query journal requires an absolute new file"
        );
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .context("creating private working query journal")?;
        let digest = Sha256::digest(serde_json::to_vec(&input.request)?);
        Ok(Self {
            file: Mutex::new(file),
            identity: Mutex::new(None),
            acknowledged: Mutex::new(None),
            subscription: Mutex::new(None),
            selected: Mutex::new(None),
            request_sha256: Sha256Digest::from_bytes(digest.into()),
            usage: Mutex::new(None),
        })
    }
    pub fn record(
        &self,
        event: Event,
        status: Option<TaskStatus>,
        drain: Option<&DrainReceipt>,
    ) -> Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| anyhow!("working query journal lock failed"))?;
        let selected = self
            .selected
            .lock()
            .map_err(|_| anyhow!("selected process journal lock failed"))?;
        serde_json::to_writer(
            &mut *file,
            &Observation {
                schema: Schema::V1,
                event,
                task_id: self
                    .acknowledged
                    .lock()
                    .map_err(|_| anyhow!("acknowledged Task journal lock failed"))?
                    .clone(),
                created_at: self
                    .identity
                    .lock()
                    .map_err(|_| anyhow!("Task journal lock failed"))?
                    .as_ref()
                    .map(|identity| identity.created_at),
                status,
                subscription_id: self
                    .subscription
                    .lock()
                    .map_err(|_| anyhow!("subscription journal lock failed"))?
                    .clone(),
                selected_process: selected.as_ref(),
                drain,
                request_sha256: self.request_sha256.clone(),
                native_usage: *self
                    .usage
                    .lock()
                    .map_err(|_| anyhow!("usage journal lock failed"))?,
            },
        )?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        Ok(())
    }
}
async fn catalog(client: &SmokeMcpClient) -> Result<BTreeSet<TaskId>> {
    let mut uri = DuckDbUsageIndexUri::new(None).to_uri()?;
    let mut ids = BTreeSet::new();
    let mut previous = None;
    for _ in 0..32 {
        let page: DuckDbUsagePage = serde_json::from_value(
            read_mcp_resource_json(client, uri.as_str())
                .await
                .map_err(|_| anyhow!("query usage catalog read failed"))?,
        )
        .map_err(|_| anyhow!("query usage catalog admission failed"))?;
        for item in page.items() {
            let id = item.task_id();
            ensure!(
                previous.is_none_or(|p| id < p) && ids.insert(id),
                "usage catalog order or duplicate differs"
            );
            previous = Some(id);
        }
        let Some(next) = page.next_cursor() else {
            return Ok(ids);
        };
        ensure!(
            previous == Some(next.after()),
            "usage continuation differs from final member"
        );
        uri = DuckDbUsageIndexUri::new(Some(next)).to_uri()?;
    }
    bail!("query usage catalog exceeded 32 pages")
}
async fn next(
    handles: &mut cleanup::Handles,
    original: &Identity,
    journal: &Journal,
) -> Result<DetailedTask> {
    let stream = handles.listener.as_mut().context("query listener absent")?;
    let update = stream
        .next()
        .await
        .map_err(|_| anyhow!("query Task notification failed"))?
        .context("query Task subscription ended")?;
    // The official Subscription validates subscription metadata and exact filter membership.
    let ServerNotification::TaskStatusNotification(update) = update else {
        bail!("unexpected query notification");
    };
    same(&update.params.task.task, original)?;
    journal.record(
        Event::TaskDelivered,
        Some(update.params.task.status()),
        None,
    )?;
    Ok(update.params.task)
}
async fn listen(
    handles: &mut cleanup::Handles,
    original: &Identity,
    journal: &Journal,
) -> Result<()> {
    ensure!(
        handles.listener.is_none(),
        "previous query listener is still owned"
    );
    let filter = SubscriptionFilter::builder()
        .task_ids([original.task_id.to_string()])
        .build();
    let listener = handles
        .client
        .as_ref()
        .unwrap()
        .listen(filter.clone())
        .await
        .map_err(|_| anyhow!("query Task listen failed"))?;
    handles.listener = Some(listener);
    ensure!(
        handles.listener.as_ref().unwrap().acknowledged() == &filter,
        "query acknowledged filter differs"
    );
    *journal
        .subscription
        .lock()
        .map_err(|_| anyhow!("subscription journal lock failed"))? =
        Some(handles.listener.as_ref().unwrap().id().clone());
    journal.record(Event::SubscriptionAcknowledged, None, None)?;
    Ok(())
}

pub(super) async fn run(
    installation: &InstalledTarget,
    database: &DuckDbDatabaseId,
    selection: &drain::DrainInput,
    input: &Input,
    output: &Path,
) -> Result<()> {
    input.admit(database)?;
    ensure!(
        input.reserved_work_context.as_str() == installation.target.operator.work_context,
        "working query requires the explicitly reserved operator Work Context"
    );
    let journal = Arc::new(Journal::create(output, input)?);
    journal.record(Event::Admitted, None, None)?;
    let (owned, registration) = cleanup::retained(journal.clone())?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(input.deadline_seconds);
    let result = tokio::time::timeout_at(
        deadline,
        exercise(installation, database, selection, input, &journal, &owned),
    )
    .await
    .context("working query recovery operation deadline")
    .and_then(|r| r);
    if result.is_err() {
        let _ = journal.record(Event::Failed, None, None);
    }
    let closed = owned.lock().await.close(&journal).await;
    if closed.is_ok() {
        registration.settled()?;
    }
    result?;
    closed?;
    journal.record(Event::Passed, Some(TaskStatus::Completed), None)?;
    Ok(())
}
async fn exercise(
    installation: &InstalledTarget,
    database: &DuckDbDatabaseId,
    selection: &drain::DrainInput,
    input: &Input,
    journal: &Journal,
    owned: &tokio::sync::Mutex<cleanup::Handles>,
) -> Result<()> {
    let mut handles = owned.lock().await;
    let token = tokio::time::timeout(Duration::from_secs(15), installation.token())
        .await
        .context("working query OAuth deadline")?
        .map_err(|_| anyhow!("working query OAuth admission failed"))?;
    handles.client = Some(
        tokio::time::timeout(
            Duration::from_secs(15),
            connect_mcp_client(installation.operator.resource.as_str(), &token),
        )
        .await
        .context("working query MCP deadline")?
        .map_err(|_| anyhow!("working query MCP admission failed"))?,
    );
    let client = handles.client.as_ref().unwrap();
    let selected = drain::select(installation, client, selection)
        .await
        .map_err(|_| anyhow!("working query process admission failed"))?;
    *journal
        .selected
        .lock()
        .map_err(|_| anyhow!("process journal lock failed"))? = Some(selected.identity());
    journal.record(Event::SelectedProcess, None, None)?;
    let schema: DuckDbDatabaseSchema = serde_json::from_value(
        read_mcp_resource_json(
            client,
            &DuckDbDatabaseUri::new(database.clone()).to_string(),
        )
        .await
        .map_err(|_| anyhow!("working query database read failed"))?,
    )
    .map_err(|_| anyhow!("working query database schema admission failed"))?;
    ensure!(
        &schema.db_id == database,
        "working query database identity differs"
    );
    let before = catalog(client).await?;
    journal.record(Event::DispatchIntent, None, None)?;
    let created = call_tool_as_task(
        client,
        "duckdb__query",
        serde_json::to_value(&input.request)?,
    )
    .await
    .map_err(|_| anyhow!("query dispatch unresolved; never redispatched"))?;
    let acknowledged = CanonicalTaskId::parse(&created.task_id)
        .map_err(|_| anyhow!("invalid acknowledged query Task ID"))?;
    *journal
        .acknowledged
        .lock()
        .map_err(|_| anyhow!("acknowledged Task journal lock failed"))? = Some(acknowledged);
    journal.record(Event::TaskAcknowledged, Some(created.status), None)?;
    let original = identity(&created)?;
    *journal
        .identity
        .lock()
        .map_err(|_| anyhow!("Task journal lock failed"))? = Some(original.clone());
    ensure!(
        created.status == TaskStatus::Working,
        "unfinished recovery unqualified: create already terminal"
    );
    listen(&mut handles, &original, journal).await?;
    let delivered = next(&mut handles, &original, journal).await?;
    working(&delivered, &original)?;
    journal.record(Event::WorkingDelivered, Some(TaskStatus::Working), None)?;
    let current = handles
        .client
        .as_ref()
        .unwrap()
        .get_task(GetTaskParams::new(original.task_id.as_str()))
        .await
        .map_err(|_| anyhow!("pre-restart current query read failed"))?
        .task;
    working(&current, &original)?;
    journal.record(Event::WorkingCurrent, Some(TaskStatus::Working), None)?;
    handles
        .close_listener(tokio::time::Instant::now() + Duration::from_secs(5))
        .await?;
    journal.record(Event::RestartIntent, Some(TaskStatus::Working), None)?;
    handles.restart = Some(Box::pin(selected.restart()));
    let restarted = handles.restart.as_mut().unwrap().await;
    handles.restart = None;
    handles.drain = Some(restarted.map_err(|_| {
        anyhow!(
            "original restart failed; partial physical facts unavailable and outcome unqualified"
        )
    })?);
    journal.record(Event::RestartObserved, None, handles.drain.as_ref())?;
    let current = handles
        .client
        .as_ref()
        .unwrap()
        .get_task(GetTaskParams::new(original.task_id.as_str()))
        .await
        .map_err(|_| anyhow!("replacement current query read failed"))?
        .task;
    working(&current, &original)?;
    journal.record(
        Event::ReplacementWorking,
        Some(TaskStatus::Working),
        handles.drain.as_ref(),
    )?;
    listen(&mut handles, &original, journal).await?;
    let mut delivered = None;
    for _ in 0..128 {
        let task = next(&mut handles, &original, journal).await?;
        match task.status() {
            TaskStatus::Working => {}
            TaskStatus::Completed => {
                delivered = Some(task);
                break;
            }
            _ => bail!("recovered query delivered unsuccessful status"),
        }
    }
    let delivered = delivered.context("recovered query exceeded 128 notifications")?;
    let client = handles.client.as_ref().unwrap();
    let current = await_task_terminal_with_timeout(
        client,
        original.task_id.as_str(),
        Duration::from_secs(15),
    )
    .await
    .map_err(|_| anyhow!("completed current query read failed"))?;
    let payload = completed(&delivered, &current, &original)?;
    let result = input.require_output(&payload)?;
    journal.record(
        Event::CompletedDelivered,
        Some(TaskStatus::Completed),
        handles.drain.as_ref(),
    )?;
    let after = catalog(client).await?;
    ensure!(
        before.is_subset(&after),
        "reserved query usage ledger lost existing members"
    );
    let added: Vec<_> = after.difference(&before).copied().collect();
    ensure!(
        added.len() == 1,
        "reserved query usage ledger must gain exactly one native Task"
    );
    let native = added[0];
    let uri = DuckDbTaskUsageUri::new(native)?.to_uri()?;
    let report: veoveo_mcp_contract::UsageReport = serde_json::from_value(
        read_mcp_resource_json(client, uri.as_str())
            .await
            .map_err(|_| anyhow!("query usage read failed"))?,
    )
    .map_err(|_| anyhow!("query usage admission failed"))?;
    require_usage(&report, native, &uri, result.rows().len(), &original)?;
    *journal
        .usage
        .lock()
        .map_err(|_| anyhow!("usage journal lock failed"))? = Some(native);
    journal.record(Event::UsageVerified, None, handles.drain.as_ref())?;
    Ok::<_, anyhow::Error>(())
}

fn require_usage(
    report: &veoveo_mcp_contract::UsageReport,
    native: TaskId,
    uri: &veoveo_types::ResourceUri,
    rows: usize,
    original: &Identity,
) -> Result<()> {
    use veoveo_mcp_contract::UsageKind;
    ensure!(
        report.task_id == native.to_string()
            && report.usage_uri == uri.as_str()
            && report.records.len() == 1
            && report.total_kind == Some(UsageKind::Actual)
            && report.total_amount.is_none()
            && report.currency.is_none(),
        "query usage parent/count/total differs"
    );
    let record = &report.records[0];
    ensure!(
        record.task_id == native.to_string()
            && record.kind == UsageKind::Actual
            && record.model_id == "duckdb/query"
            && record.quantity == Some(rows as f64)
            && record.unit.as_deref() == Some("row")
            && record.amount.is_none()
            && record.currency.is_none()
            && record.source_id.is_none()
            && record.provider_job_id.is_none()
            && record.recorded_at >= original.created_at,
        "query usage facts or creation-time correlation differs"
    );
    let details: DuckDbUsageDetails = serde_json::from_value(record.metadata.clone())
        .map_err(|_| anyhow!("query usage details admission failed"))?;
    ensure!(
        details
            == DuckDbUsageDetails::Query {
                result: DuckDbQueryUsage::Inline {
                    rows_returned: rows,
                    truncated: false
                }
            },
        "query usage operation/output differs"
    );
    Ok(())
}

#[cfg(test)]
#[path = "recovery/tests.rs"]
mod tests;
