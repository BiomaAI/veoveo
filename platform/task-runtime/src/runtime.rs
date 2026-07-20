use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    AccessLevel, AccessSubject, InvocationAuthority, InvocationProvenance,
    WorkContextMembershipLevel,
};
use veoveo_platform_store::{
    ArtifactGrantSubjectKind, GrantPermission, InvocationAuthorityRecord,
    InvocationMode as StoreInvocationMode, LiveStream, OpenObject, OutboxDraft, OutboxEventRecord,
    PlatformStore, PlatformTable, RecoveryClass as StoreRecoveryClass, TaskId, TaskInputRecord,
    TaskRecord, TaskStatus as StoreTaskStatus, WorkContextInitialGrantRecord,
    WorkContextMembershipLevel as StoreMembershipLevel, deterministic_principal_id,
    deterministic_tenant_id, deterministic_work_context_id,
};

use crate::types::{
    ClaimedTask, CreateTask, CreateTaskResult, RecoveryClass, RecoveryReport, RequestEnvelope,
    TaskError, TaskFailure, TaskInputExchange, TaskInputRequest, TaskInputSubmission, TaskOwner,
    TaskPayloadState, TaskRetentionPin, TaskRuntimeConfig, TaskSnapshot, TaskTransition,
    TaskUpdate, TaskUpdateCursor, failure_to_open_object, open_object_to_value, parse_task_id,
    record_to_snapshot, value_to_open_object,
};

const EVENT_SCHEMA_VERSION: i64 = 2;
const DEFAULT_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_TRANSACTION_ATTEMPTS: u32 = 8;

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct TaskContent {
    tenant: RecordId,
    owner: RecordId,
    work_context: RecordId,
    initiator: Option<RecordId>,
    invocation_mode: StoreInvocationMode,
    delegation_id: Option<String>,
    policy_revision: String,
    authority: InvocationAuthorityRecord,
    profile: RecordId,
    server: RecordId,
    task_type: String,
    status: StoreTaskStatus,
    recovery_class: StoreRecoveryClass,
    request: OpenObject,
    progress: f64,
    result: Option<OpenObject>,
    error: Option<OpenObject>,
    result_artifact: Option<RecordId>,
    idempotency_key: Option<String>,
    lease_owner: Option<String>,
    lease_expires_at: Option<DateTime<Utc>>,
    cancel_requested_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    retention_expires_at: Option<DateTime<Utc>>,
    retention_pins: Vec<String>,
    search_text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct IdempotencyContent {
    task: RecordId,
    tenant: RecordId,
    owner: RecordId,
    server: RecordId,
    key: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct TaskInputContent {
    task: RecordId,
    request_key: String,
    request: OpenObject,
    response: Option<OpenObject>,
    created_at: DateTime<Utc>,
    responded_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Deserialize, SurrealValue)]
struct TaskUpdateBaseline {
    cursor: Option<i64>,
    tasks: Vec<TaskRecord>,
}

pub type TaskUpdateStream =
    Pin<Box<dyn Stream<Item = Result<TaskUpdate, TaskError>> + Send + 'static>>;

struct Worker {
    cancellation: CancellationToken,
    join: JoinHandle<()>,
}

#[derive(Clone)]
pub struct TaskRuntime {
    store: PlatformStore,
    server: String,
    worker_id: String,
    workers: Arc<Mutex<HashMap<TaskId, Worker>>>,
    changed: watch::Sender<u64>,
}

impl TaskRuntime {
    pub async fn connect(
        config: TaskRuntimeConfig,
        server: impl Into<String>,
        worker_id: impl Into<String>,
    ) -> Result<Self, TaskError> {
        let store_config = veoveo_platform_store::StoreConfig::builder(
            config.endpoint,
            config.namespace,
            config.database,
            config.credentials,
        )
        .build()?;
        let store = PlatformStore::connect(store_config).await?;
        Ok(Self::new(store, server, worker_id))
    }

    pub fn new(
        store: PlatformStore,
        server: impl Into<String>,
        worker_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            server: server.into(),
            worker_id: worker_id.into(),
            workers: Arc::new(Mutex::new(HashMap::new())),
            changed: watch::channel(0).0,
        }
    }

    pub fn platform_store(&self) -> &PlatformStore {
        &self.store
    }

    pub fn server(&self) -> &str {
        &self.server
    }

    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    pub async fn create(&self, draft: CreateTask) -> Result<CreateTaskResult, TaskError> {
        if draft.server != self.server {
            return Err(TaskError::WrongServer(draft.server));
        }
        if draft.owner.authority.tenant.as_str() != draft.owner.tenant_key() {
            return Err(TaskError::InvalidAuthority(
                "task owner and Work Context belong to different tenants".into(),
            ));
        }
        if let Some(key) = draft.idempotency_key.as_deref()
            && let Some(snapshot) = self.idempotent_task(&draft.owner, key).await?
        {
            return Ok(CreateTaskResult {
                snapshot,
                created: false,
            });
        }

        self.store
            .ensure_identity(
                draft.owner.tenant_key(),
                &draft.owner.principal_key,
                &draft.owner.issuer,
                &draft.owner.subject,
                draft.owner.principal_kind,
            )
            .await?;

        let task_id = draft.task_id;
        let record = task_id.record_id();
        let now = Utc::now();
        let retention = draft
            .ttl_ms
            .and_then(|ttl| TimeDelta::try_milliseconds(ttl as i64))
            .or_else(|| TimeDelta::from_std(DEFAULT_RETENTION).ok())
            .map(|ttl| now + ttl);
        let envelope = RequestEnvelope {
            input: draft.request.clone(),
            owner: draft.owner.clone(),
            status_message: Some("accepted; queued".to_owned()),
            ttl_ms: draft.ttl_ms,
            poll_interval_ms: draft.poll_interval_ms,
        };
        let content = TaskContent {
            tenant: tenant_record(&draft.owner)?,
            owner: owner_record(&draft.owner)?,
            work_context: deterministic_work_context_id(
                draft.owner.tenant_key(),
                draft.owner.authority.work_context.as_str(),
            )?
            .record_id(),
            initiator: authority_initiator_record(&draft.owner)?,
            invocation_mode: store_invocation_mode(&draft.owner.authority),
            delegation_id: authority_delegation_id(&draft.owner.authority),
            policy_revision: draft.owner.authority.policy_revision.to_string(),
            authority: authority_record(&draft.owner.authority),
            profile: RecordId::new("profile", draft.owner.profile.clone()),
            server: RecordId::new("mcp_server", self.server.clone()),
            task_type: draft.task_type.clone(),
            status: StoreTaskStatus::Queued,
            recovery_class: draft.recovery_class.into(),
            request: envelope.into_open_object()?,
            progress: 0.0,
            result: None,
            error: None,
            result_artifact: None,
            idempotency_key: draft.idempotency_key.clone(),
            lease_owner: None,
            lease_expires_at: None,
            cancel_requested_at: None,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            retention_expires_at: retention,
            retention_pins: draft
                .retention_pins
                .iter()
                .map(ToString::to_string)
                .collect(),
            search_text: format!(
                "{} {} {}",
                self.server, draft.task_type, draft.owner.principal_key
            ),
        };
        let initial_snapshot = TaskSnapshot {
            task_id,
            owner: draft.owner.clone(),
            server: self.server.clone(),
            task_type: draft.task_type.clone(),
            request: draft.request.clone(),
            recovery_class: draft.recovery_class,
            status: StoreTaskStatus::Queued,
            status_message: Some("accepted; queued".to_owned()),
            progress: 0.0,
            result: None,
            error: None,
            idempotency_key: draft.idempotency_key.clone(),
            lease_owner: None,
            lease_expires_at: None,
            cancel_requested_at: None,
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
            retention_expires_at: retention,
            retention_pins: draft.retention_pins.clone(),
            ttl_ms: draft.ttl_ms,
            poll_interval_ms: draft.poll_interval_ms,
        };
        let outbox = task_event(&initial_snapshot, "task.created")?;

        if let Some(key) = draft.idempotency_key.as_deref() {
            let idempotency = idempotency_record(&draft.owner, &self.server, key);
            let link = IdempotencyContent {
                task: record,
                tenant: tenant_record(&draft.owner)?,
                owner: owner_record(&draft.owner)?,
                server: RecordId::new("mcp_server", self.server.clone()),
                key: key.to_owned(),
                created_at: now,
            };
            for attempt in 0..MAX_TRANSACTION_ATTEMPTS {
                let result = self
                    .store
                    .client()
                    .query(
                        "BEGIN TRANSACTION; CREATE ONLY $idempotency CONTENT $link RETURN NONE; CREATE ONLY $task CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;",
                    )
                    .bind(("idempotency", idempotency.clone()))
                    .bind(("link", link.clone()))
                    .bind(("task", task_id.record_id()))
                    .bind(("content", content.clone()))
                    .bind(("outbox", outbox.clone()))
                    .await
                    .and_then(|response| response.check());
                match result {
                    Ok(_) => break,
                    Err(error) => {
                        if let Some(snapshot) = self.idempotent_task(&draft.owner, key).await? {
                            return Ok(CreateTaskResult {
                                snapshot,
                                created: false,
                            });
                        }
                        if is_retryable_transaction_failure(&error)
                            && attempt + 1 < MAX_TRANSACTION_ATTEMPTS
                        {
                            transaction_retry_backoff(attempt).await;
                            continue;
                        }
                        return Err(TaskError::Database(error));
                    }
                }
            }
        } else {
            self.store
                .client()
                .query(
                    "BEGIN TRANSACTION; CREATE ONLY $task CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;",
                )
                .bind(("task", record))
                .bind(("content", content))
                .bind(("outbox", outbox))
                .await?
                .check()?;
        }

        let snapshot = self
            .get(task_id.to_string().as_str())
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        self.note_change();
        Ok(CreateTaskResult {
            snapshot,
            created: true,
        })
    }

    pub async fn get(&self, task_id: &str) -> Result<Option<TaskSnapshot>, TaskError> {
        let task_id = parse_task_id(task_id)?;
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM ONLY $task;")
            .bind(("task", task_id.record_id()))
            .await?
            .check()?;
        let record: Option<TaskRecord> = response.take(0)?;
        let snapshot = record.map(record_to_snapshot).transpose()?;
        Ok(snapshot.filter(|snapshot| snapshot.server == self.server))
    }

    pub async fn list(&self) -> Result<Vec<TaskSnapshot>, TaskError> {
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM task WHERE server = $server ORDER BY created_at ASC;")
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .await?
            .check()?;
        let records: Vec<TaskRecord> = response.take(0)?;
        records.into_iter().map(record_to_snapshot).collect()
    }

    pub async fn list_for_owner(&self, owner: &TaskOwner) -> Result<Vec<TaskSnapshot>, TaskError> {
        let mut response = self
            .store
            .client()
            .query(
                "SELECT * FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner AND profile = $profile ORDER BY created_at ASC;",
            )
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .bind(("tenant", tenant_record(owner)?))
            .bind(("owner", owner_record(owner)?))
            .bind(("profile", RecordId::new("profile", owner.profile.clone())))
            .await?
            .check()?;
        let records: Vec<TaskRecord> = response.take(0)?;
        records
            .into_iter()
            .map(record_to_snapshot)
            .filter(|snapshot| {
                snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.owner.data_labels.is_subset(&owner.data_labels))
                    .unwrap_or(true)
            })
            .collect()
    }

    pub async fn owner(&self, task_id: &str) -> Result<Option<TaskOwner>, TaskError> {
        Ok(self.get(task_id).await?.map(|snapshot| snapshot.owner))
    }

    /// Adopt a pin after task creation only for explicit repair workflows.
    /// Normal delivery guarantees must place the pin in `CreateTask` so task
    /// creation and retention protection are one atomic write.
    pub async fn adopt_retention_pin_for_repair(
        &self,
        task_id: &str,
        pin: &TaskRetentionPin,
    ) -> Result<TaskSnapshot, TaskError> {
        let task_id = parse_task_id(task_id)?;
        let mut response = self
            .store
            .client()
            .query(
                "UPDATE ONLY $task SET retention_pins += $pin WHERE server = $server AND !(retention_pins CONTAINS $pin) RETURN AFTER;",
            )
            .bind(("task", task_id.record_id()))
            .bind(("pin", pin.as_str().to_owned()))
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(0)?;
        if let Some(updated) = updated {
            return record_to_snapshot(updated);
        }
        self.get(&task_id.to_string())
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))
    }

    /// Release one consumer's retention guarantee after its result delivery
    /// acknowledgement is durable. Repeating an acknowledgement is harmless.
    pub async fn acknowledge_retention_pin(
        &self,
        task_id: &str,
        pin: &TaskRetentionPin,
    ) -> Result<TaskSnapshot, TaskError> {
        let task_id = parse_task_id(task_id)?;
        let mut response = self
            .store
            .client()
            .query(
                "UPDATE ONLY $task SET retention_pins -= $pin WHERE server = $server AND retention_pins CONTAINS $pin RETURN AFTER;",
            )
            .bind(("task", task_id.record_id()))
            .bind(("pin", pin.as_str().to_owned()))
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(0)?;
        if let Some(updated) = updated {
            return record_to_snapshot(updated);
        }
        self.get(&task_id.to_string())
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))
    }

    pub async fn request_input(
        &self,
        task_id: &str,
        key: &str,
        request: TaskInputRequest,
    ) -> Result<TaskInputExchange, TaskError> {
        validate_input_key(key)?;
        validate_input_method(&request.method)?;
        let current = self
            .get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_owned()))?;
        if !matches!(
            current.status,
            StoreTaskStatus::Queued | StoreTaskStatus::Running | StoreTaskStatus::Waiting
        ) {
            return Err(TaskError::InvalidTransition {
                from: current.status,
                to: StoreTaskStatus::Waiting,
            });
        }
        let now = Utc::now();
        if current.lease_owner.as_deref() != Some(&self.worker_id)
            || current.lease_expires_at.is_none_or(|expiry| expiry <= now)
        {
            return Err(TaskError::LeaseHeld(task_id.to_owned()));
        }

        let input_id = task_input_record(current.task_id, key);
        let content = TaskInputContent {
            task: current.task_id.record_id(),
            request_key: key.to_owned(),
            request: task_input_request_to_open_object(&request)?,
            response: None,
            created_at: now,
            responded_at: None,
        };
        let envelope = RequestEnvelope {
            input: current.request.clone(),
            owner: current.owner.clone(),
            status_message: Some("input required".to_owned()),
            ttl_ms: current.ttl_ms,
            poll_interval_ms: current.poll_interval_ms,
        };
        let mut event_snapshot = current.clone();
        event_snapshot.status = StoreTaskStatus::Waiting;
        event_snapshot.status_message = Some("input required".to_owned());
        event_snapshot.updated_at = now;
        let event = task_event(&event_snapshot, "task.input_requested")?;
        let result = self
            .store
            .client()
            .query(
                "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = 'waiting', request = $request, updated_at = $now WHERE updated_at = $expected_updated_at AND status IN ['queued', 'running', 'waiting'] AND server = $server AND tenant = $tenant AND owner = $owner AND lease_owner = $worker AND lease_expires_at > $now RETURN AFTER); IF $updated = NONE { THROW 'task input transition conflict'; }; CREATE ONLY $input CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;",
            )
            .bind(("task", current.task_id.record_id()))
            .bind(("request", envelope.into_open_object()?))
            .bind(("now", now))
            .bind(("expected_updated_at", current.updated_at))
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .bind(("tenant", tenant_record(&current.owner)?))
            .bind(("owner", owner_record(&current.owner)?))
            .bind(("worker", self.worker_id.clone()))
            .bind(("input", input_id.clone()))
            .bind(("content", content))
            .bind(("event", event))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if self.input_exchange_by_id(input_id).await?.is_some() {
                return Err(TaskError::DuplicateInputKey(key.to_owned()));
            }
            if self
                .get(task_id)
                .await?
                .is_none_or(|snapshot| snapshot.updated_at != current.updated_at)
            {
                return Err(TaskError::Conflict(task_id.to_owned()));
            }
            return Err(TaskError::Database(error));
        }
        self.note_change();
        self.input_exchange_by_id(task_input_record(current.task_id, key))
            .await?
            .ok_or_else(|| TaskError::InvalidRecord("task input readback is missing".to_owned()))
    }

    pub async fn outstanding_inputs(
        &self,
        task_id: &str,
    ) -> Result<BTreeMap<String, TaskInputRequest>, TaskError> {
        let task_id = parse_task_id(task_id)?;
        self.get(&task_id.to_string())
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        let mut response = self
            .store
            .client()
            .query(
                "SELECT * FROM task_input WHERE task = $task AND response = NONE ORDER BY created_at ASC;",
            )
            .bind(("task", task_id.record_id()))
            .await?
            .check()?;
        let records: Vec<TaskInputRecord> = response.take(0)?;
        records
            .into_iter()
            .map(task_input_record_to_exchange)
            .map(|exchange| exchange.map(|exchange| (exchange.key, exchange.request)))
            .collect()
    }

    pub async fn submit_input_responses(
        &self,
        task_id: &str,
        responses: BTreeMap<String, BTreeMap<String, serde_json::Value>>,
    ) -> Result<TaskInputSubmission, TaskError> {
        let current = self
            .get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_owned()))?;
        if current.is_terminal() || current.status == StoreTaskStatus::CancelRequested {
            return Err(TaskError::InvalidTransition {
                from: current.status,
                to: StoreTaskStatus::Running,
            });
        }
        let mut submission = TaskInputSubmission::default();
        for (key, response_value) in responses {
            validate_input_key(&key)?;
            let mut attempt = 0;
            let accepted = loop {
                let now = Utc::now();
                let mut event_snapshot = current.clone();
                event_snapshot.updated_at = now;
                let event = task_event(&event_snapshot, "task.input_received")?;
                let result = self
                    .store
                    .client()
                    .query(
                        "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $input SET response = $response, responded_at = $now WHERE task = $task AND response = NONE RETURN AFTER); IF $updated != NONE { LET $task_updated = (UPDATE ONLY $task SET updated_at = $now WHERE server = $server AND status IN ['queued', 'running', 'waiting'] RETURN AFTER); IF $task_updated = NONE { THROW 'task cannot accept input'; }; CREATE outbox_event CONTENT $event RETURN NONE; }; RETURN $updated; COMMIT TRANSACTION;",
                    )
                    .bind(("input", task_input_record(current.task_id, &key)))
                    .bind(("response", OpenObject::new(response_value.clone())))
                    .bind(("now", now))
                    .bind(("task", current.task_id.record_id()))
                    .bind(("server", RecordId::new("mcp_server", self.server.clone())))
                    .bind(("event", event))
                    .await
                    .and_then(|response| response.check());
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

    pub async fn live_updates(&self) -> Result<TaskUpdateStream, TaskError> {
        let wake = self.outbox_wake().await?;
        let (cursor, snapshots) = self.update_baseline().await?;
        Ok(self.task_update_stream(wake, cursor, snapshots, false))
    }

    /// Resume strictly after a previously delivered durable cursor. The LIVE
    /// query is opened before replay so writes racing the replay remain queued
    /// as wake signals; the outbox, not LIVE delivery, supplies every update.
    pub async fn live_updates_after(
        &self,
        cursor: TaskUpdateCursor,
    ) -> Result<TaskUpdateStream, TaskError> {
        let wake = self.outbox_wake().await?;
        Ok(self.task_update_stream(wake, cursor, Vec::new(), true))
    }

    async fn outbox_wake(&self) -> Result<LiveStream<OutboxEventRecord>, TaskError> {
        Ok(self
            .store
            .live::<OutboxEventRecord>(PlatformTable::OutboxEvent)
            .await?)
    }

    async fn update_baseline(&self) -> Result<(TaskUpdateCursor, Vec<TaskSnapshot>), TaskError> {
        let mut response = self
            .store
            .client()
            .query(
                "RETURN { cursor: array::first((SELECT VALUE sequence FROM outbox_event WHERE available_at <= $now ORDER BY sequence DESC LIMIT 1)), tasks: (SELECT * FROM task WHERE server = $server ORDER BY created_at ASC) };",
            )
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .bind(("now", Utc::now()))
            .await?
            .check()?;
        let baseline: TaskUpdateBaseline = response
            .take::<Option<TaskUpdateBaseline>>(0)?
            .ok_or_else(|| {
                TaskError::InvalidRecord("task update baseline is missing".to_owned())
            })?;
        let snapshots = baseline
            .tasks
            .into_iter()
            .map(record_to_snapshot)
            .collect::<Result<_, _>>()?;
        Ok((
            TaskUpdateCursor::from_sequence(baseline.cursor.unwrap_or(0))
                .expect("outbox sequences are non-negative"),
            snapshots,
        ))
    }

    fn task_update_stream(
        &self,
        mut wake: LiveStream<OutboxEventRecord>,
        mut cursor: TaskUpdateCursor,
        initial: Vec<TaskSnapshot>,
        replay_immediately: bool,
    ) -> TaskUpdateStream {
        let runtime = self.clone();
        Box::pin(async_stream::stream! {
            for snapshot in initial {
                yield Ok(TaskUpdate { cursor, snapshot });
            }

            let mut must_replay = replay_immediately;
            loop {
                if !must_replay {
                    match wake.next().await {
                        Some(Ok(_)) => {}
                        Some(Err(_)) | None => match runtime.outbox_wake().await {
                            Ok(reconnected) => wake = reconnected,
                            Err(error) => {
                                yield Err(error);
                                break;
                            }
                        },
                    }
                }
                must_replay = false;

                match runtime.replay_task_updates(cursor).await {
                    Ok(updates) => {
                        for update in updates {
                            cursor = update.cursor;
                            yield Ok(update);
                        }
                    }
                    Err(error) => {
                        yield Err(error);
                        break;
                    }
                }
            }
        })
    }

    async fn replay_task_updates(
        &self,
        mut cursor: TaskUpdateCursor,
    ) -> Result<Vec<TaskUpdate>, TaskError> {
        let mut updates = Vec::new();
        loop {
            let page = self.store.read_outbox(cursor.sequence(), 1_000).await?;
            let event_count = page.events.len();
            for event in page.events {
                cursor = TaskUpdateCursor::from_sequence(event.sequence).ok_or_else(|| {
                    TaskError::InvalidRecord("outbox sequence is negative".to_owned())
                })?;
                if event.aggregate_type != "task" {
                    continue;
                }
                let snapshot = task_snapshot_from_event(&event)?;
                if snapshot.server == self.server {
                    updates.push(TaskUpdate { cursor, snapshot });
                }
            }
            if event_count < 1_000 {
                break;
            }
        }
        Ok(updates)
    }

    pub async fn claim(
        &self,
        task_id: &str,
        lease_duration: Duration,
    ) -> Result<ClaimedTask, TaskError> {
        if lease_duration.is_zero() {
            return Err(TaskError::InvalidRecord(
                "task lease duration must be greater than zero".to_owned(),
            ));
        }
        let snapshot = self
            .get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_owned()))?;
        if snapshot.server != self.server {
            return Err(TaskError::WrongServer(task_id.to_owned()));
        }
        let now = Utc::now();
        if snapshot.lease_expires_at.is_some_and(|expiry| {
            expiry > now && snapshot.lease_owner.as_deref() != Some(&self.worker_id)
        }) {
            return Err(TaskError::LeaseHeld(task_id.to_owned()));
        }
        if snapshot.is_terminal() || snapshot.status == StoreTaskStatus::CancelRequested {
            return Err(TaskError::InvalidTransition {
                from: snapshot.status,
                to: StoreTaskStatus::Running,
            });
        }
        let lease_expires_at = now
            + TimeDelta::from_std(lease_duration)
                .map_err(|_| TaskError::InvalidRecord("lease duration is too large".to_owned()))?;
        let task = snapshot.task_id.record_id();
        let mut event_snapshot = snapshot.clone();
        event_snapshot.status = StoreTaskStatus::Running;
        event_snapshot.status_message = Some("claimed for execution".to_owned());
        event_snapshot.lease_owner = Some(self.worker_id.clone());
        event_snapshot.lease_expires_at = Some(lease_expires_at);
        event_snapshot.started_at = snapshot.started_at.or(Some(now));
        event_snapshot.updated_at = now;
        let event = task_event(&event_snapshot, "task.claimed")?;
        let request = RequestEnvelope {
            input: snapshot.request.clone(),
            owner: snapshot.owner.clone(),
            status_message: event_snapshot.status_message.clone(),
            ttl_ms: snapshot.ttl_ms,
            poll_interval_ms: snapshot.poll_interval_ms,
        };
        let mut response = self
            .store
            .client()
            .query(
                "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = 'running', request = $request, lease_owner = $worker, lease_expires_at = $lease_expires, started_at = started_at ?? $now, updated_at = $now WHERE status = $expected AND updated_at = $expected_updated_at AND (lease_expires_at = NONE OR lease_expires_at <= $now OR lease_owner = $worker) RETURN AFTER); IF $updated != NONE { CREATE outbox_event CONTENT $event RETURN NONE; }; RETURN $updated; COMMIT TRANSACTION;",
            )
            .bind(("task", task))
            .bind(("worker", self.worker_id.clone()))
            .bind(("request", request.into_open_object()?))
            .bind(("lease_expires", lease_expires_at))
            .bind(("now", now))
            .bind(("expected", snapshot.status))
            .bind(("expected_updated_at", snapshot.updated_at))
            .bind(("event", event))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(3)?;
        let snapshot = updated
            .map(record_to_snapshot)
            .transpose()?
            .ok_or_else(|| TaskError::Conflict(task_id.to_owned()))?;
        self.note_change();
        Ok(ClaimedTask {
            snapshot,
            lease_owner: self.worker_id.clone(),
            lease_expires_at,
        })
    }

    pub async fn renew_lease(
        &self,
        task_id: &str,
        lease_duration: Duration,
    ) -> Result<TaskSnapshot, TaskError> {
        if lease_duration.is_zero() {
            return Err(TaskError::InvalidRecord(
                "task lease duration must be greater than zero".to_owned(),
            ));
        }
        let task_id = parse_task_id(task_id)?;
        self.get(&task_id.to_string())
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        let now = Utc::now();
        let lease_expires = now
            + TimeDelta::from_std(lease_duration)
                .map_err(|_| TaskError::InvalidRecord("lease duration is too large".to_owned()))?;
        let mut response = self
            .store
            .client()
            .query(
                "UPDATE ONLY $task SET lease_expires_at = $lease_expires WHERE lease_owner = $worker AND lease_expires_at > $now AND status IN ['running', 'waiting', 'cancel_requested'] RETURN AFTER;",
            )
            .bind(("task", task_id.record_id()))
            .bind(("worker", self.worker_id.clone()))
            .bind(("lease_expires", lease_expires))
            .bind(("now", now))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(0)?;
        updated
            .map(record_to_snapshot)
            .transpose()?
            .ok_or_else(|| TaskError::LeaseHeld(task_id.to_string()))
    }

    pub async fn transition(
        &self,
        task_id: &str,
        transition: TaskTransition,
    ) -> Result<TaskSnapshot, TaskError> {
        let current = self
            .get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_owned()))?;
        self.transition_if_current(&current, transition).await
    }

    /// Compare-and-set transition using the caller's durable snapshot. This is
    /// useful when work was derived from that snapshot and must not publish
    /// over a newer progress/result update from another replica.
    pub async fn transition_if_current(
        &self,
        current: &TaskSnapshot,
        transition: TaskTransition,
    ) -> Result<TaskSnapshot, TaskError> {
        let task_id = current.task_id.to_string();
        if current.server != self.server {
            return Err(TaskError::WrongServer(task_id));
        }
        let durable = self
            .get(&task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.clone()))?;
        if durable.status != current.status || durable.updated_at != current.updated_at {
            return Err(TaskError::Conflict(task_id));
        }
        let next = transition.status();
        if !allowed_transition(current.status, next) {
            return Err(TaskError::InvalidTransition {
                from: current.status,
                to: next,
            });
        }
        let progress = transition.progress(current.progress);
        if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
            return Err(TaskError::InvalidProgress);
        }
        let now = Utc::now();
        let control_transition = next == StoreTaskStatus::CancelRequested;
        let expired_cancellation = current.status == StoreTaskStatus::CancelRequested
            && next == StoreTaskStatus::Cancelled;
        if !control_transition
            && !expired_cancellation
            && durable.lease_owner.as_deref() != Some(&self.worker_id)
        {
            return Err(TaskError::LeaseHeld(task_id));
        }
        let terminal = matches!(
            next,
            StoreTaskStatus::Succeeded | StoreTaskStatus::Failed | StoreTaskStatus::Cancelled
        );
        let message = transition.message();
        let mut envelope = RequestEnvelope {
            input: current.request.clone(),
            owner: current.owner.clone(),
            status_message: Some(message.clone()),
            ttl_ms: current.ttl_ms,
            poll_interval_ms: current.poll_interval_ms,
        };
        envelope.status_message = Some(message.clone());
        let event_type = format!("task.{}", status_name(next));
        let event_snapshot = transitioned_snapshot(&durable, &transition, now);
        let event = task_event(&event_snapshot, &event_type)?;
        let mut response = self
            .store
            .client()
            .query(
                "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = $next, request = $request, progress = $progress, result = $result, error = $error, cancel_requested_at = $cancel_requested_at, completed_at = $completed_at, lease_owner = IF $terminal { NONE } ELSE { lease_owner }, lease_expires_at = IF $terminal { NONE } ELSE { lease_expires_at }, updated_at = $now WHERE status = $expected AND updated_at = $expected_updated_at AND server = $server AND tenant = $tenant AND owner = $owner AND ($control_transition OR (lease_owner = $worker AND lease_expires_at > $now) OR ($expired_cancellation AND (lease_expires_at = NONE OR lease_expires_at <= $now))) RETURN AFTER); IF $updated != NONE { CREATE outbox_event CONTENT $event RETURN NONE; }; RETURN $updated; COMMIT TRANSACTION;",
            )
            .bind(("task", current.task_id.record_id()))
            .bind(("next", next))
            .bind(("request", envelope.into_open_object()?))
            .bind(("progress", progress))
            .bind(("result", transition.result().map(value_to_open_object)))
            .bind(("error", transition.failure().as_ref().map(failure_to_open_object)))
            .bind((
                "cancel_requested_at",
                if next == StoreTaskStatus::CancelRequested {
                    Some(now)
                } else {
                    current.cancel_requested_at
                },
            ))
            .bind(("completed_at", terminal.then_some(now)))
            .bind(("terminal", terminal))
            .bind(("now", now))
            .bind(("expected", current.status))
            .bind(("expected_updated_at", current.updated_at))
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .bind(("tenant", tenant_record(&current.owner)?))
            .bind(("owner", owner_record(&current.owner)?))
            .bind(("worker", self.worker_id.clone()))
            .bind(("control_transition", control_transition))
            .bind(("expired_cancellation", expired_cancellation))
            .bind(("event", event))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(3)?;
        let snapshot = updated
            .map(record_to_snapshot)
            .transpose()?
            .ok_or(TaskError::Conflict(task_id))?;
        self.note_change();
        Ok(snapshot)
    }

    pub async fn cancel(&self, task_id: &str) -> Result<TaskSnapshot, TaskError> {
        loop {
            let current = self
                .get(task_id)
                .await?
                .ok_or_else(|| TaskError::NotFound(task_id.to_owned()))?;
            if current.is_terminal() {
                return Ok(current);
            }
            let requested = if current.status == StoreTaskStatus::CancelRequested {
                current
            } else {
                match self
                    .transition_if_current(&current, TaskTransition::CancelRequested)
                    .await
                {
                    Ok(requested) => requested,
                    Err(TaskError::Conflict(_)) => continue,
                    Err(error) => return Err(error),
                }
            };
            if let Some(worker) = self.workers.lock().await.get(&requested.task_id) {
                worker.cancellation.cancel();
            }
            if requested.lease_owner.is_none() {
                match self
                    .transition_if_current(&requested, TaskTransition::Cancelled)
                    .await
                {
                    Ok(cancelled) => return Ok(cancelled),
                    Err(TaskError::Conflict(_)) => continue,
                    Err(error) => return Err(error),
                }
            }
            return Ok(requested);
        }
    }

    pub async fn is_cancel_requested(&self, task_id: &str) -> Result<bool, TaskError> {
        Ok(self
            .get(task_id)
            .await?
            .is_some_and(|snapshot| snapshot.status == StoreTaskStatus::CancelRequested))
    }

    pub async fn payload_state(&self, task_id: &str) -> Result<TaskPayloadState, TaskError> {
        let Some(snapshot) = self.get(task_id).await? else {
            return Ok(TaskPayloadState::Unknown);
        };
        Ok(match snapshot.status {
            StoreTaskStatus::Succeeded => snapshot
                .result
                .map(TaskPayloadState::Completed)
                .unwrap_or_else(|| {
                    TaskPayloadState::Failed(TaskFailure::new(
                        "missing_result",
                        "completed task has no durable result",
                    ))
                }),
            StoreTaskStatus::Failed => TaskPayloadState::Failed(
                snapshot
                    .error
                    .unwrap_or_else(|| TaskFailure::new("task_failed", "task failed")),
            ),
            StoreTaskStatus::Cancelled => TaskPayloadState::Cancelled,
            _ => TaskPayloadState::Running,
        })
    }

    pub async fn await_payload_state(&self, task_id: &str) -> Result<TaskPayloadState, TaskError> {
        let mut changed = self.changed.subscribe();
        loop {
            let state = self.payload_state(task_id).await?;
            if !matches!(state, TaskPayloadState::Running) {
                return Ok(state);
            }
            changed.mark_unchanged();
            // This watch is only a latency hint. The durable row is polled so
            // transitions from another replica or a missed LIVE event cannot
            // strand a durable payload wait.
            let _ = tokio::time::timeout(Duration::from_millis(500), changed.changed()).await;
        }
    }

    pub async fn register_worker(
        &self,
        task_id: &str,
        cancellation: CancellationToken,
        join: JoinHandle<()>,
    ) -> Result<(), TaskError> {
        let task_id = parse_task_id(task_id)?;
        self.workers
            .lock()
            .await
            .insert(task_id, Worker { cancellation, join });
        Ok(())
    }

    pub async fn reap_workers(&self) {
        self.workers
            .lock()
            .await
            .retain(|_, worker| !worker.join.is_finished());
    }

    pub async fn recover(&self) -> Result<RecoveryReport, TaskError> {
        let mut report = RecoveryReport::default();
        let tasks = self.list().await?;
        for task in tasks {
            if task
                .lease_expires_at
                .is_some_and(|expiry| expiry > Utc::now())
            {
                continue;
            }
            match task.status {
                StoreTaskStatus::Queued => {
                    if task.recovery_class == RecoveryClass::WebhookWait {
                        if let Some(waiting) = recovery_result(self.force_waiting(&task).await)? {
                            report.webhook_waiting.push(waiting);
                        }
                    } else {
                        report.resumable.push(task);
                    }
                }
                StoreTaskStatus::CancelRequested => {
                    if let Some(cancelled) = recovery_result(
                        self.transition_if_current(&task, TaskTransition::Cancelled)
                            .await,
                    )? {
                        report.cancelled.push(cancelled);
                    }
                }
                StoreTaskStatus::Running | StoreTaskStatus::Waiting => match task.recovery_class {
                    RecoveryClass::Resume => {
                        if let Some(reset) = recovery_result(self.reset_for_recovery(&task).await)?
                        {
                            report.resumable.push(reset);
                        }
                    }
                    RecoveryClass::WebhookWait => {
                        let waiting = if task.status == StoreTaskStatus::Waiting {
                            Some(task)
                        } else {
                            recovery_result(self.force_waiting(&task).await)?
                        };
                        if let Some(waiting) = waiting {
                            report.webhook_waiting.push(waiting);
                        }
                    }
                    RecoveryClass::InterruptedIndeterminate => {
                        if let Some(failed) = recovery_result(
                            self.force_failed(&task, TaskFailure::interrupted_indeterminate())
                                .await,
                        )? {
                            report.failed_indeterminate.push(failed);
                        }
                    }
                },
                StoreTaskStatus::Succeeded
                | StoreTaskStatus::Failed
                | StoreTaskStatus::Cancelled => {}
            }
        }
        Ok(report)
    }

    pub async fn prune_expired(&self) -> Result<Vec<TaskId>, TaskError> {
        let now = Utc::now();
        let mut response = self
            .store
            .client()
            .query(
                "BEGIN TRANSACTION; LET $expired = (SELECT VALUE id FROM task WHERE retention_expires_at != NONE AND retention_expires_at <= $now AND array::len(retention_pins) = 0 AND status IN ['succeeded', 'failed', 'cancelled']); DELETE task_idempotency WHERE task IN $expired RETURN NONE; DELETE task_input WHERE task IN $expired RETURN NONE; LET $deleted = (DELETE task WHERE id IN $expired RETURN BEFORE); RETURN $deleted; COMMIT TRANSACTION;",
            )
            .bind(("now", now))
            .await?
            .check()?;
        let records: Vec<TaskRecord> = response.take(5)?;
        records
            .into_iter()
            .map(|record| record_to_snapshot(record).map(|snapshot| snapshot.task_id))
            .collect()
    }

    async fn idempotent_task(
        &self,
        owner: &TaskOwner,
        key: &str,
    ) -> Result<Option<TaskSnapshot>, TaskError> {
        let id = idempotency_record(owner, &self.server, key);
        let mut response = self
            .store
            .client()
            .query("SELECT VALUE task FROM ONLY $id;")
            .bind(("id", id))
            .await?
            .check()?;
        let task: Option<RecordId> = response.take(0)?;
        let Some(task) = task else {
            return Ok(None);
        };
        let task_id = crate::types::task_id_from_record(&task)?;
        self.get(&task_id.to_string()).await
    }

    async fn input_exchange_by_id(
        &self,
        input_id: RecordId,
    ) -> Result<Option<TaskInputExchange>, TaskError> {
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM ONLY $input;")
            .bind(("input", input_id))
            .await?
            .check()?;
        response
            .take::<Option<TaskInputRecord>>(0)?
            .map(task_input_record_to_exchange)
            .transpose()
    }

    async fn reset_for_recovery(&self, task: &TaskSnapshot) -> Result<TaskSnapshot, TaskError> {
        self.force_status(
            task,
            StoreTaskStatus::Queued,
            "reclaimed after process restart",
            None,
        )
        .await
    }

    async fn force_waiting(&self, task: &TaskSnapshot) -> Result<TaskSnapshot, TaskError> {
        self.force_status(
            task,
            StoreTaskStatus::Waiting,
            "waiting for provider webhook",
            None,
        )
        .await
    }

    async fn force_failed(
        &self,
        task: &TaskSnapshot,
        failure: TaskFailure,
    ) -> Result<TaskSnapshot, TaskError> {
        let message = failure.message.clone();
        self.force_status(task, StoreTaskStatus::Failed, &message, Some(failure))
            .await
    }

    async fn force_status(
        &self,
        task: &TaskSnapshot,
        status: StoreTaskStatus,
        message: &str,
        failure: Option<TaskFailure>,
    ) -> Result<TaskSnapshot, TaskError> {
        let now = Utc::now();
        let envelope = RequestEnvelope {
            input: task.request.clone(),
            owner: task.owner.clone(),
            status_message: Some(message.to_owned()),
            ttl_ms: task.ttl_ms,
            poll_interval_ms: task.poll_interval_ms,
        };
        let terminal = status == StoreTaskStatus::Failed;
        let mut event_snapshot = task.clone();
        event_snapshot.status = status;
        event_snapshot.status_message = Some(message.to_owned());
        event_snapshot.error = failure.clone();
        event_snapshot.lease_owner = None;
        event_snapshot.lease_expires_at = None;
        event_snapshot.completed_at = terminal.then_some(now);
        event_snapshot.updated_at = now;
        let event = task_event(&event_snapshot, &format!("task.{}", status_name(status)))?;
        let mut response = self
            .store
            .client()
            .query(
                "BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = $status, request = $request, error = $error, lease_owner = NONE, lease_expires_at = NONE, completed_at = $completed_at, updated_at = $now WHERE status = $expected AND updated_at = $expected_updated_at AND (lease_expires_at = NONE OR lease_expires_at <= $now) RETURN AFTER); IF $updated != NONE { CREATE outbox_event CONTENT $event RETURN NONE; }; RETURN $updated; COMMIT TRANSACTION;",
            )
            .bind(("task", task.task_id.record_id()))
            .bind(("status", status))
            .bind(("request", envelope.into_open_object()?))
            .bind(("error", failure.as_ref().map(failure_to_open_object)))
            .bind(("completed_at", terminal.then_some(now)))
            .bind(("now", now))
            .bind(("expected", task.status))
            .bind(("expected_updated_at", task.updated_at))
            .bind(("event", event))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(3)?;
        let snapshot = updated
            .map(record_to_snapshot)
            .transpose()?
            .ok_or_else(|| TaskError::Conflict(task.task_id.to_string()))?;
        self.note_change();
        Ok(snapshot)
    }

    fn note_change(&self) {
        self.changed.send_modify(|version| *version += 1);
    }
}

fn is_retryable_transaction_failure(error: &surrealdb::Error) -> bool {
    matches!(
        error.query_details(),
        Some(surrealdb::types::QueryError::TransactionConflict)
    ) || error.message().starts_with("Transaction conflict:")
        || error
            .message()
            .contains("not executed due to a failed transaction")
}

async fn transaction_retry_backoff(attempt: u32) {
    tokio::time::sleep(Duration::from_millis(1_u64 << attempt)).await;
}

fn recovery_result<T>(result: Result<T, TaskError>) -> Result<Option<T>, TaskError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(TaskError::Conflict(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

fn allowed_transition(from: StoreTaskStatus, to: StoreTaskStatus) -> bool {
    match from {
        StoreTaskStatus::Queued => matches!(
            to,
            StoreTaskStatus::Running
                | StoreTaskStatus::Waiting
                | StoreTaskStatus::CancelRequested
                | StoreTaskStatus::Failed
        ),
        StoreTaskStatus::Running => matches!(
            to,
            StoreTaskStatus::Running
                | StoreTaskStatus::Waiting
                | StoreTaskStatus::Succeeded
                | StoreTaskStatus::Failed
                | StoreTaskStatus::CancelRequested
        ),
        StoreTaskStatus::Waiting => matches!(
            to,
            StoreTaskStatus::Running
                | StoreTaskStatus::Succeeded
                | StoreTaskStatus::Failed
                | StoreTaskStatus::CancelRequested
        ),
        StoreTaskStatus::CancelRequested => {
            matches!(to, StoreTaskStatus::Cancelled | StoreTaskStatus::Failed)
        }
        StoreTaskStatus::Succeeded | StoreTaskStatus::Failed | StoreTaskStatus::Cancelled => false,
    }
}

fn status_name(status: StoreTaskStatus) -> &'static str {
    match status {
        StoreTaskStatus::Queued => "queued",
        StoreTaskStatus::Running => "running",
        StoreTaskStatus::Waiting => "waiting",
        StoreTaskStatus::Succeeded => "succeeded",
        StoreTaskStatus::Failed => "failed",
        StoreTaskStatus::CancelRequested => "cancel_requested",
        StoreTaskStatus::Cancelled => "cancelled",
    }
}

pub(crate) fn authority_record(authority: &InvocationAuthority) -> InvocationAuthorityRecord {
    let (invocation_mode, initiator_key, delegation_id) = match &authority.provenance {
        InvocationProvenance::Direct { initiator } => (
            StoreInvocationMode::Direct,
            Some(initiator.to_string()),
            None,
        ),
        InvocationProvenance::Delegated {
            initiator,
            delegation_id,
        } => (
            StoreInvocationMode::Delegated,
            Some(initiator.to_string()),
            Some(delegation_id.to_string()),
        ),
        InvocationProvenance::Automated => (StoreInvocationMode::Automated, None, None),
    };
    let (owner_kind, owner_key) = subject_record(&authority.output_policy.owner);
    InvocationAuthorityRecord {
        context_key: authority.work_context.to_string(),
        membership: store_membership(authority.membership),
        policy_revision: authority.policy_revision.to_string(),
        owner_kind,
        owner_key,
        initial_grants: authority
            .output_policy
            .initial_grants
            .iter()
            .map(|grant| {
                let (subject_kind, subject_key) = subject_record(&grant.subject);
                WorkContextInitialGrantRecord {
                    subject_kind,
                    subject_key,
                    permission: store_permission(grant.level),
                }
            })
            .collect(),
        classification: authority
            .output_policy
            .classification
            .as_ref()
            .map(ToString::to_string),
        data_labels: authority
            .output_policy
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        invocation_mode,
        initiator_key,
        delegation_id,
    }
}

fn subject_record(subject: &AccessSubject) -> (ArtifactGrantSubjectKind, String) {
    match subject {
        AccessSubject::Principal(principal) => {
            (ArtifactGrantSubjectKind::Principal, principal.to_string())
        }
        AccessSubject::Group(group) => (ArtifactGrantSubjectKind::Group, group.to_string()),
    }
}

fn store_membership(level: WorkContextMembershipLevel) -> StoreMembershipLevel {
    match level {
        WorkContextMembershipLevel::Viewer => StoreMembershipLevel::Viewer,
        WorkContextMembershipLevel::Contributor => StoreMembershipLevel::Contributor,
        WorkContextMembershipLevel::Custodian => StoreMembershipLevel::Custodian,
        WorkContextMembershipLevel::Owner => StoreMembershipLevel::Owner,
    }
}

fn store_permission(level: AccessLevel) -> GrantPermission {
    match level {
        AccessLevel::Read => GrantPermission::Read,
        AccessLevel::Write => GrantPermission::Write,
        AccessLevel::Admin => GrantPermission::Admin,
    }
}

fn store_invocation_mode(authority: &InvocationAuthority) -> StoreInvocationMode {
    match &authority.provenance {
        InvocationProvenance::Direct { .. } => StoreInvocationMode::Direct,
        InvocationProvenance::Delegated { .. } => StoreInvocationMode::Delegated,
        InvocationProvenance::Automated => StoreInvocationMode::Automated,
    }
}

fn authority_delegation_id(authority: &InvocationAuthority) -> Option<String> {
    match &authority.provenance {
        InvocationProvenance::Delegated { delegation_id, .. } => Some(delegation_id.to_string()),
        InvocationProvenance::Direct { .. } | InvocationProvenance::Automated => None,
    }
}

fn authority_initiator_record(owner: &TaskOwner) -> Result<Option<RecordId>, TaskError> {
    owner
        .authority
        .provenance
        .initiator()
        .map(|initiator| {
            deterministic_principal_id(owner.tenant_key(), initiator.as_str())
                .map(|principal| principal.record_id())
                .map_err(TaskError::from)
        })
        .transpose()
}

fn tenant_record(owner: &TaskOwner) -> Result<RecordId, TaskError> {
    Ok(deterministic_tenant_id(owner.tenant_key())?.record_id())
}

fn owner_record(owner: &TaskOwner) -> Result<RecordId, TaskError> {
    Ok(deterministic_principal_id(owner.tenant_key(), &owner.principal_key)?.record_id())
}

fn transitioned_snapshot(
    current: &TaskSnapshot,
    transition: &TaskTransition,
    now: DateTime<Utc>,
) -> TaskSnapshot {
    let mut snapshot = current.clone();
    let next = transition.status();
    let terminal = matches!(
        next,
        StoreTaskStatus::Succeeded | StoreTaskStatus::Failed | StoreTaskStatus::Cancelled
    );
    snapshot.status = next;
    snapshot.status_message = Some(transition.message());
    snapshot.progress = transition.progress(current.progress);
    snapshot.result = transition.result();
    snapshot.error = transition.failure();
    if next == StoreTaskStatus::CancelRequested {
        snapshot.cancel_requested_at = Some(now);
    }
    snapshot.completed_at = terminal.then_some(now);
    snapshot.updated_at = now;
    if terminal {
        snapshot.lease_owner = None;
        snapshot.lease_expires_at = None;
    }
    snapshot
}

fn task_event(snapshot: &TaskSnapshot, event_type: &str) -> Result<OutboxDraft, TaskError> {
    let payload = BTreeMap::from([("snapshot".to_owned(), serde_json::to_value(snapshot)?)]);
    Ok(OutboxDraft::now(
        Some(tenant_record(&snapshot.owner)?),
        "task",
        snapshot.task_id.to_string(),
        event_type,
        EVENT_SCHEMA_VERSION,
        OpenObject::new(payload),
    ))
}

#[derive(Deserialize)]
struct TaskEventPayload {
    snapshot: TaskSnapshot,
}

fn task_snapshot_from_event(event: &OutboxEventRecord) -> Result<TaskSnapshot, TaskError> {
    if event.schema_version != EVENT_SCHEMA_VERSION {
        return Err(TaskError::InvalidRecord(format!(
            "task outbox event {} has schema version {}, expected {}",
            event.sequence, event.schema_version, EVENT_SCHEMA_VERSION
        )));
    }
    let payload: TaskEventPayload =
        serde_json::from_value(open_object_to_value(event.payload.clone()))?;
    if event.aggregate_id != payload.snapshot.task_id.to_string() {
        return Err(TaskError::InvalidRecord(format!(
            "task outbox event {} aggregate id does not match its snapshot",
            event.sequence
        )));
    }
    Ok(payload.snapshot)
}

fn idempotency_record(owner: &TaskOwner, server: &str, key: &str) -> RecordId {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(
        format!(
            "{}\0{}\0{}\0{}\0{}",
            owner.tenant_key(),
            owner.principal_key,
            owner.profile,
            server,
            key
        )
        .as_bytes(),
    );
    let key = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    RecordId::new("task_idempotency", key)
}

fn validate_input_key(key: &str) -> Result<(), TaskError> {
    if key.is_empty() || key.len() > 256 || key.chars().any(char::is_control) {
        return Err(TaskError::InvalidInputKey);
    }
    Ok(())
}

fn validate_input_method(method: &str) -> Result<(), TaskError> {
    if method.is_empty() || method.len() > 256 || method.chars().any(char::is_control) {
        return Err(TaskError::InvalidRecord(
            "task input method is empty, too long, or contains a control character".to_owned(),
        ));
    }
    Ok(())
}

fn task_input_record(task_id: TaskId, key: &str) -> RecordId {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{task_id}\0{key}").as_bytes());
    let key = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    RecordId::new("task_input", key)
}

fn task_input_request_to_open_object(
    request: &TaskInputRequest,
) -> Result<OpenObject, serde_json::Error> {
    let serde_json::Value::Object(values) = serde_json::to_value(request)? else {
        unreachable!("TaskInputRequest serializes as an object")
    };
    Ok(OpenObject::new(values.into_iter().collect()))
}

fn task_input_record_to_exchange(record: TaskInputRecord) -> Result<TaskInputExchange, TaskError> {
    let request = serde_json::from_value(open_object_to_value(record.request))?;
    let response = record.response.map(OpenObject::into_map);
    Ok(TaskInputExchange {
        key: record.request_key,
        request,
        response,
        created_at: record.created_at,
        responded_at: record.responded_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_mcp_contract::{
        PolicyVersion, PrincipalId, TenantId, WorkContextId, WorkContextOutputPolicy,
    };

    fn direct_authority(principal: &str, tenant: &str) -> InvocationAuthority {
        let principal = PrincipalId::new(principal).unwrap();
        InvocationAuthority {
            work_context: WorkContextId::new("mission").unwrap(),
            tenant: TenantId::new(tenant).unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::new("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(principal.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: Default::default(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: principal,
            },
        }
    }

    #[test]
    fn transition_matrix_is_fail_closed() {
        assert!(allowed_transition(
            StoreTaskStatus::Queued,
            StoreTaskStatus::Running
        ));
        assert!(allowed_transition(
            StoreTaskStatus::Running,
            StoreTaskStatus::Succeeded
        ));
        assert!(allowed_transition(
            StoreTaskStatus::CancelRequested,
            StoreTaskStatus::Cancelled
        ));
        assert!(!allowed_transition(
            StoreTaskStatus::Succeeded,
            StoreTaskStatus::Running
        ));
        assert!(!allowed_transition(
            StoreTaskStatus::Cancelled,
            StoreTaskStatus::Succeeded
        ));
    }

    #[test]
    fn idempotency_scope_includes_owner_profile_tenant_and_server() {
        let owner = TaskOwner {
            principal_key: "principal-a".to_owned(),
            principal_kind: veoveo_platform_store::PrincipalKind::User,
            issuer: "https://issuer.example".to_owned(),
            subject: "subject-a".to_owned(),
            profile: "operator".to_owned(),
            tenant_key: Some("tenant-a".to_owned()),
            data_labels: Default::default(),
            authority: direct_authority("principal-a", "tenant-a"),
        };
        assert_eq!(
            idempotency_record(&owner, "timeseries", "request-1"),
            idempotency_record(&owner, "timeseries", "request-1")
        );
        assert_ne!(
            idempotency_record(&owner, "timeseries", "request-1"),
            idempotency_record(&owner, "optimization", "request-1")
        );
    }
}
