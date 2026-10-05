use veoveo_platform_store::TaskRequestRecord;
mod context_scope;
mod history;
mod input_responses;
mod owner_query;
mod owner_reads;
mod owner_subscriptions;
mod subscriptions;
mod task_pages;
mod usage;

pub use owner_query::OwnerTaskQuery;
pub use owner_subscriptions::OwnerTaskSubscription;
pub use usage::{TaskUsageAccess, TaskUsagePage};

use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use veoveo_platform_store::task_record_id;

use chrono::{DateTime, Datelike, TimeDelta, Utc};
use futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use veoveo_platform_store::{
    ArtifactGrantSubjectKind, GrantPermission, InvocationAuthorityRecord,
    InvocationMode as StoreInvocationMode, OpenObject, PlatformStore, PlatformTable,
    RecoveryClass as StoreRecoveryClass, TaskInputRecord, TaskRecord,
    TaskStatus as StoreTaskStatus, WorkContextInitialGrantRecord,
    WorkContextMembershipLevel as StoreMembershipLevel, deterministic_principal_id,
    deterministic_tenant_id, deterministic_work_context_id,
};
use veoveo_types::TaskId;
use veoveo_types::{AccessLevel, InvocationAuthority, WorkContextMembershipLevel};
use veoveo_types::{AccessSubject, InvocationProvenance};

use crate::types::{
    CreateTask, CreateTaskResult, RecoveryClass, TaskError, TaskFailure, TaskInputExchange,
    TaskInputRequest, TaskOwner, TaskPayloadState, TaskRetentionPin, TaskRuntimeConfig,
    TaskSnapshot, TaskTransition, TaskUpdate, TaskUpdateCursor, failure_to_open_object,
    open_object_to_value, record_to_snapshot, validate_task_id,
};

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
    #[surreal(wrap)]
    task_type: veoveo_types::TaskTypeName,
    status: StoreTaskStatus,
    recovery_class: StoreRecoveryClass,
    request: TaskRequestRecord,
    owner_context: veoveo_platform_store::TaskOwnerRecord,
    progress: f64,
    result: Option<veoveo_platform_store::TaskResultRecord>,
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

pub type TaskUpdateStream =
    Pin<Box<dyn Stream<Item = Result<TaskUpdate, TaskError>> + Send + 'static>>;

struct Worker {
    cancellation: CancellationToken,
    join: JoinHandle<()>,
}

#[derive(Clone)]
pub struct TaskRuntime {
    pub(crate) contributions: crate::contributions::ContributionRegistry,
    store: PlatformStore,
    server: String,
    worker_id: String,
    workers: Arc<Mutex<HashMap<TaskId, Worker>>>,
    changed: watch::Sender<u64>,
    subscription_wake: Arc<subscriptions::SharedWake>,
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
            contributions: Default::default(),
            store,
            server: server.into(),
            worker_id: worker_id.into(),
            workers: Arc::new(Mutex::new(HashMap::new())),
            changed: watch::channel(0).0,
            subscription_wake: Default::default(),
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
        validate_task_id(draft.task_id)?;
        self.check_contribution(&draft.task_type)?;
        if draft.server != self.server {
            return Err(TaskError::WrongServer(draft.server));
        }
        if draft.owner.authority.tenant.as_str() != draft.owner.tenant_key() {
            return Err(TaskError::InvalidAuthority(
                "task owner and Work Context belong to different tenants".into(),
            ));
        }
        let owner_context = veoveo_platform_store::TaskOwnerRecord::try_from(&draft.owner)?;
        let now = Utc::now();
        let duration = match draft.ttl_ms {
            Some(ttl) => i64::try_from(ttl)
                .ok()
                .and_then(TimeDelta::try_milliseconds),
            None => TimeDelta::from_std(DEFAULT_RETENTION).ok(),
        }
        .ok_or_else(|| {
            TaskError::InvalidRecord("Task TTL exceeds supported deadline range".into())
        })?;
        let retention = Some(
            now.checked_add_signed(duration)
                .filter(|deadline| deadline.year() <= 9999)
                .ok_or_else(|| {
                    TaskError::InvalidRecord("Task TTL exceeds supported deadline range".into())
                })?,
        );
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
        let record = task_record_id(task_id);
        let contribution = self.creation_contribution(&draft, now)?;
        let contribution_sql = contribution.sql(true)?;
        let envelope = TaskRequestRecord {
            input: draft.request.clone(),
            status_message: Some("Queued".to_owned()),
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
            request: envelope,
            owner_context,
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
                let query = self
                    .store
                    .client()
                    .query(if contribution_sql.is_empty() {
                        include_str!("../queries/create_idempotent.surql")
                    } else {
                        include_str!("../queries/create_idempotent_contribution.surql")
                    })
                    .bind(("idempotency", idempotency.clone()))
                    .bind(("link", link.clone()))
                    .bind(("task", task_record_id(task_id)))
                    .bind(("content", content.clone()));
                let result = contribution
                    .bind(query, task_id, &draft.task_type, now)
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
            let query = self
                .store
                .client()
                .query(if contribution_sql.is_empty() {
                    include_str!("../queries/create.surql")
                } else {
                    include_str!("../queries/create_contribution.surql")
                })
                .bind(("task", record))
                .bind(("content", content));
            contribution
                .bind(query, task_id, &draft.task_type, now)
                .await?
                .check()?;
        }

        let snapshot = self
            .get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        self.note_change();
        Ok(CreateTaskResult {
            snapshot,
            created: true,
        })
    }

    /// Read native identity without a text conversion. Public reads use `for_owner`.
    /// ```compile_fail
    /// use veoveo_task_runtime::TaskRuntime;
    /// async fn raw_identity(runtime: &TaskRuntime) {
    ///     runtime.get("01983da0-0000-7000-8000-000000000001").await;
    /// }
    /// ```
    pub async fn get(&self, task_id: TaskId) -> Result<Option<TaskSnapshot>, TaskError> {
        let task_id = validate_task_id(task_id)?;
        let mut response = self
            .store
            .client()
            .query(include_str!("../queries/runtime/get.surql"))
            .bind(("task", task_record_id(task_id)))
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .await?
            .check()?;
        let records: Vec<TaskRecord> = response.take(0)?;
        let snapshot = records
            .into_iter()
            .next()
            .map(record_to_snapshot)
            .transpose()?;
        if let Some(snapshot) = &snapshot {
            self.check_contribution(&snapshot.task_type)?;
        }
        Ok(snapshot)
    }

    pub async fn list(&self) -> Result<Vec<TaskSnapshot>, TaskError> {
        let mut response = self
            .store
            .client()
            .query(include_str!("../queries/runtime/list.surql"))
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .await?
            .check()?;
        let records: Vec<TaskRecord> = response.take(0)?;
        records.into_iter().map(record_to_snapshot).collect()
    }

    pub async fn owner(&self, task_id: TaskId) -> Result<Option<TaskOwner>, TaskError> {
        Ok(self.get(task_id).await?.map(|snapshot| snapshot.owner))
    }

    /// Adopt a pin after task creation only for explicit repair workflows.
    /// Normal delivery guarantees must place the pin in `CreateTask` so task
    /// creation and retention protection are one atomic write.
    pub async fn adopt_retention_pin_for_repair(
        &self,
        task_id: TaskId,
        pin: &TaskRetentionPin,
    ) -> Result<TaskSnapshot, TaskError> {
        let task_id = validate_task_id(task_id)?;
        let mut response = self
            .store
            .client()
            .query(include_str!(
                "../queries/runtime/adopt_retention_pin_for_repair.surql"
            ))
            .bind(("task", task_record_id(task_id)))
            .bind(("pin", pin.as_str().to_owned()))
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(0)?;
        if let Some(updated) = updated {
            return record_to_snapshot(updated);
        }
        self.get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))
    }

    /// Release one consumer's retention guarantee after its result delivery
    /// acknowledgement is durable. Repeating an acknowledgement is harmless.
    pub async fn acknowledge_retention_pin(
        &self,
        task_id: TaskId,
        pin: &TaskRetentionPin,
    ) -> Result<TaskSnapshot, TaskError> {
        let task_id = validate_task_id(task_id)?;
        let mut response = self
            .store
            .client()
            .query(include_str!(
                "../queries/runtime/acknowledge_retention_pin.surql"
            ))
            .bind(("task", task_record_id(task_id)))
            .bind(("pin", pin.as_str().to_owned()))
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .await?
            .check()?;
        let updated: Option<TaskRecord> = response.take(0)?;
        if let Some(updated) = updated {
            return record_to_snapshot(updated);
        }
        self.get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))
    }

    pub async fn request_input(
        &self,
        task_id: TaskId,
        key: &str,
        request: TaskInputRequest,
    ) -> Result<TaskInputExchange, TaskError> {
        validate_input_key(key)?;
        validate_input_method(&request.method)?;
        let current = self
            .get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
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
            return Err(TaskError::LeaseHeld(task_id.to_string()));
        }

        let input_id = task_input_record(current.task_id, key);
        let content = TaskInputContent {
            task: task_record_id(current.task_id),
            request_key: key.to_owned(),
            request: task_input_request_to_open_object(&request)?,
            response: None,
            created_at: now,
            responded_at: None,
        };
        let envelope = TaskRequestRecord {
            input: current.request.clone(),
            status_message: Some("Waiting for input".to_owned()),
            ttl_ms: current.ttl_ms,
            poll_interval_ms: current.poll_interval_ms,
        };

        let result = self
            .store
            .client()
            .query(include_str!("../queries/runtime/request_input.surql"))
            .bind(("task", task_record_id(current.task_id)))
            .bind(("request", envelope.into_value()))
            .bind(("now", now))
            .bind(("expected_updated_at", current.updated_at))
            .bind(("expected_request", TaskRequestRecord::from(&current)))
            .bind((
                "expected_owner_context",
                veoveo_platform_store::TaskOwnerRecord::try_from(&current.owner)?,
            ))
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .bind(("tenant", tenant_record(&current.owner)?))
            .bind(("owner", owner_record(&current.owner)?))
            .bind(("worker", self.worker_id.clone()))
            .bind(("input", input_id.clone()))
            .bind(("content", content))
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
                return Err(TaskError::Conflict(task_id.to_string()));
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
        task_id: TaskId,
    ) -> Result<BTreeMap<String, TaskInputRequest>, TaskError> {
        let task_id = validate_task_id(task_id)?;
        self.get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
        let mut response = self
            .store
            .client()
            .query(include_str!("../queries/runtime/outstanding_inputs.surql"))
            .bind(("task", task_record_id(task_id)))
            .await?
            .check()?;
        let records: Vec<TaskInputRecord> = response.take(0)?;
        records
            .into_iter()
            .map(task_input_record_to_exchange)
            .map(|exchange| exchange.map(|exchange| (exchange.key, exchange.request)))
            .collect()
    }

    pub async fn transition(
        &self,
        task_id: TaskId,
        transition: TaskTransition,
    ) -> Result<TaskSnapshot, TaskError> {
        let current = self
            .get(task_id)
            .await?
            .ok_or_else(|| TaskError::NotFound(task_id.to_string()))?;
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
        self.transition_selected(current, transition, None).await
    }

    async fn selected_snapshot(
        &self,
        task: TaskId,
        selection: Option<&OwnerTaskQuery>,
    ) -> Result<TaskSnapshot, TaskError> {
        let snapshot = match selection {
            Some(query) => query.get(task).await?,
            None => self.get(task).await?,
        };
        let snapshot = snapshot.ok_or_else(|| TaskError::NotFound(task.to_string()))?;
        self.check_contribution(&snapshot.task_type)?;
        Ok(snapshot)
    }

    async fn transition_selected(
        &self,
        current: &TaskSnapshot,
        transition: TaskTransition,
        selection: Option<&OwnerTaskQuery>,
    ) -> Result<TaskSnapshot, TaskError> {
        let task_id = current.task_id.to_string();
        if current.server != self.server {
            return Err(TaskError::WrongServer(task_id));
        }
        let durable = self.selected_snapshot(current.task_id, selection).await?;
        if durable.status != current.status || durable.updated_at != current.updated_at {
            return Err(TaskError::Conflict(task_id));
        }
        let next = transition.status();
        // An observation claim preserves Queued, and cancellation can race an
        // already dispatched effect. Its qualified owner may publish the known
        // successful outcome after committing the domain result.
        let provider_completion = current.recovery_class == RecoveryClass::ProviderWait
            && next == StoreTaskStatus::Succeeded
            && matches!(
                current.status,
                StoreTaskStatus::Queued | StoreTaskStatus::CancelRequested
            );
        let provider_progress = current.recovery_class == RecoveryClass::ProviderWait
            && current.status == StoreTaskStatus::Waiting
            && next == StoreTaskStatus::Waiting;
        if !provider_completion && !provider_progress && !allowed_transition(current.status, next) {
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
        let expired_cancellation = current.recovery_class != RecoveryClass::ProviderWait
            && current.status == StoreTaskStatus::CancelRequested
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
        let contribution = match &transition {
            TaskTransition::Succeeded { result, .. } => self.settlement_contribution(
                current,
                crate::TaskSettlement::Succeeded { result },
                now,
            )?,
            TaskTransition::Failed(failure) => self.settlement_contribution(
                current,
                crate::TaskSettlement::Failed { failure },
                now,
            )?,
            TaskTransition::Cancelled => {
                self.settlement_contribution(current, crate::TaskSettlement::Cancelled, now)?
            }
            _ => crate::TaskContribution::none(),
        };
        let contribution_sql = contribution.sql(false)?;
        let message = transition.message();
        let mut envelope = TaskRequestRecord {
            input: current.request.clone(),
            status_message: Some(message.clone()),
            ttl_ms: current.ttl_ms,
            poll_interval_ms: current.poll_interval_ms,
        };
        envelope.status_message = Some(message.clone());

        let query = self
            .store
            .client()
            .query(
                match (
                    selection.map(OwnerTaskQuery::selection),
                    !contribution_sql.is_empty(),
                ) {
                    (None, false) => include_str!("../queries/transition/trusted.surql"),
                    (None, true) => {
                        include_str!("../queries/transition/trusted_contribution.surql")
                    }
                    (Some(owner_query::OwnerSelection::Owner), false) => {
                        include_str!("../queries/transition/owner.surql")
                    }
                    (Some(owner_query::OwnerSelection::Owner), true) => {
                        include_str!("../queries/transition/owner_contribution.surql")
                    }
                    (Some(owner_query::OwnerSelection::Operations), false) => {
                        include_str!("../queries/transition/operations.surql")
                    }
                    (Some(owner_query::OwnerSelection::Operations), true) => {
                        include_str!("../queries/transition/operations_contribution.surql")
                    }
                    (Some(owner_query::OwnerSelection::Context), false) => {
                        include_str!("../queries/transition/context.surql")
                    }
                    (Some(owner_query::OwnerSelection::Context), true) => {
                        include_str!("../queries/transition/context_contribution.surql")
                    }
                    (Some(owner_query::OwnerSelection::ContextOperations), false) => {
                        include_str!("../queries/transition/context_operations.surql")
                    }
                    (Some(owner_query::OwnerSelection::ContextOperations), true) => {
                        include_str!("../queries/transition/context_operations_contribution.surql")
                    }
                },
            )
            .bind(("task", task_record_id(current.task_id)))
            .bind(("next", next))
            .bind(("request", envelope.into_value()))
            .bind(("progress", progress))
            .bind((
                "result",
                transition
                    .result()
                    .map(veoveo_platform_store::TaskResultRecord::new),
            ))
            .bind((
                "error",
                transition.failure().as_ref().map(failure_to_open_object),
            ))
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
            .bind(("expected_request", TaskRequestRecord::from(current)))
            .bind((
                "expected_owner_context",
                veoveo_platform_store::TaskOwnerRecord::try_from(&current.owner)?,
            ))
            .bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .bind(("tenant", tenant_record(&current.owner)?))
            .bind(("owner", owner_record(&current.owner)?))
            .bind(("worker", self.worker_id.clone()))
            .bind(("control_transition", control_transition))
            .bind(("expired_cancellation", expired_cancellation));
        let query = match selection {
            Some(selection) => selection.bind(query)?,
            None => query,
        };
        let mut response = contribution
            .bind(
                query,
                current.task_id,
                &current.task_type,
                current.created_at,
            )
            .await?;
        if let Some(error) =
            veoveo_platform_store::primary_transaction_error(response.take_errors())
        {
            return Err(TaskError::Database(error));
        }
        let updated: Option<TaskRecord> = response.take(3)?;
        let snapshot = updated
            .map(record_to_snapshot)
            .transpose()?
            .ok_or(TaskError::Conflict(task_id))?;
        self.note_change();
        Ok(snapshot)
    }

    pub async fn cancel(&self, task_id: TaskId) -> Result<TaskSnapshot, TaskError> {
        self.cancel_selected(validate_task_id(task_id)?, None).await
    }

    async fn cancel_selected(
        &self,
        task_id: TaskId,
        selection: Option<&OwnerTaskQuery>,
    ) -> Result<TaskSnapshot, TaskError> {
        loop {
            let current = self.selected_snapshot(task_id, selection).await?;
            if current.is_terminal() {
                return Ok(current);
            }
            let requested = if current.status == StoreTaskStatus::CancelRequested {
                current
            } else {
                match self
                    .transition_selected(&current, TaskTransition::CancelRequested, selection)
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
            if requested.lease_owner.is_none()
                && requested.recovery_class != RecoveryClass::ProviderWait
            {
                match self
                    .transition_selected(&requested, TaskTransition::Cancelled, selection)
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

    pub async fn is_cancel_requested(&self, task_id: TaskId) -> Result<bool, TaskError> {
        Ok(self
            .get(task_id)
            .await?
            .is_some_and(|snapshot| snapshot.status == StoreTaskStatus::CancelRequested))
    }

    pub async fn payload_state(&self, task_id: TaskId) -> Result<TaskPayloadState, TaskError> {
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

    pub async fn await_payload_state(
        &self,
        task_id: TaskId,
    ) -> Result<TaskPayloadState, TaskError> {
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
        task_id: TaskId,
        cancellation: CancellationToken,
        join: JoinHandle<()>,
    ) -> Result<(), TaskError> {
        let task_id = validate_task_id(task_id)?;
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

    pub async fn prune_expired(&self) -> Result<Vec<TaskId>, TaskError> {
        let now = Utc::now();
        let mut response = self
            .store
            .client()
            .query(include_str!("../queries/runtime/prune_expired.surql"))
            .bind(("now", now))
            .await?
            .check()?;
        let records: Vec<TaskRecord> = response.take(0)?;
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
            .query(include_str!("../queries/runtime/idempotent_task.surql"))
            .bind(("id", id))
            .await?
            .check()?;
        let task: Option<RecordId> = response.take(0)?;
        let Some(task) = task else {
            return Ok(None);
        };
        let task_id = crate::types::task_id_from_record(&task)?;
        self.get(task_id).await
    }

    async fn input_exchange_by_id(
        &self,
        input_id: RecordId,
    ) -> Result<Option<TaskInputExchange>, TaskError> {
        let mut response = self
            .store
            .client()
            .query(include_str!(
                "../queries/runtime/input_exchange_by_id.surql"
            ))
            .bind(("input", input_id))
            .await?
            .check()?;
        response
            .take::<Option<TaskInputRecord>>(0)?
            .map(task_input_record_to_exchange)
            .transpose()
    }

    pub(super) fn note_change(&self) {
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

pub(super) fn recovery_result<T>(result: Result<T, TaskError>) -> Result<Option<T>, TaskError> {
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
    use veoveo_types::WorkContextOutputPolicy;
    use veoveo_types::{PolicyVersion, PrincipalId, TenantId, WorkContextId};

    fn direct_authority(principal: &str, tenant: &str) -> InvocationAuthority {
        let principal = PrincipalId::parse(principal).unwrap();
        InvocationAuthority {
            work_context: WorkContextId::parse("mission").unwrap(),
            tenant: TenantId::parse(tenant).unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::parse("r1").unwrap(),
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
