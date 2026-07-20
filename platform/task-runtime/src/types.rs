use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use secrecy::SecretString;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use surrealdb::types::{RecordId, RecordIdKey};
use veoveo_mcp_contract::InvocationAuthority;
use veoveo_platform_store::{
    OpenObject, PrincipalKind, RecoveryClass as StoreRecoveryClass, StoreAuthLevel,
    StoreCredentials, TaskId, TaskRecord, TaskStatus as StoreTaskStatus,
    deterministic_principal_id, deterministic_tenant_id, deterministic_work_context_id,
};

const INSTALLATION_TENANT: &str = "installation";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryClass {
    /// Deterministic work can be reclaimed and rerun from its persisted input.
    Resume,
    /// Completion is accepted only through a provider webhook.
    WebhookWait,
    /// A process crash during execution makes the result unknowable.
    InterruptedIndeterminate,
}

#[derive(Clone)]
pub struct TaskRuntimeConfig {
    pub endpoint: String,
    pub namespace: String,
    pub database: String,
    pub credentials: StoreCredentials,
}

impl TaskRuntimeConfig {
    pub fn new(
        endpoint: impl Into<String>,
        namespace: impl Into<String>,
        database: impl Into<String>,
        auth_level: StoreAuthLevel,
        username: impl Into<String>,
        password: impl Into<SecretString>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            namespace: namespace.into(),
            database: database.into(),
            credentials: StoreCredentials::new(auth_level, username, password),
        }
    }
}

impl std::fmt::Debug for TaskRuntimeConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TaskRuntimeConfig")
            .field("endpoint", &self.endpoint)
            .field("namespace", &self.namespace)
            .field("database", &self.database)
            .field("credentials", &self.credentials)
            .finish()
    }
}

impl From<RecoveryClass> for StoreRecoveryClass {
    fn from(value: RecoveryClass) -> Self {
        match value {
            RecoveryClass::Resume => Self::Resume,
            RecoveryClass::WebhookWait => Self::WebhookWait,
            RecoveryClass::InterruptedIndeterminate => Self::InterruptedIndeterminate,
        }
    }
}

impl From<StoreRecoveryClass> for RecoveryClass {
    fn from(value: StoreRecoveryClass) -> Self {
        match value {
            StoreRecoveryClass::Resume => Self::Resume,
            StoreRecoveryClass::WebhookWait => Self::WebhookWait,
            StoreRecoveryClass::InterruptedIndeterminate => Self::InterruptedIndeterminate,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskOwner {
    pub principal_key: String,
    pub principal_kind: PrincipalKind,
    pub issuer: String,
    pub subject: String,
    pub profile: String,
    pub tenant_key: Option<String>,
    #[serde(default)]
    pub data_labels: BTreeSet<String>,
    pub authority: InvocationAuthority,
}

impl TaskOwner {
    pub fn tenant_key(&self) -> &str {
        self.tenant_key.as_deref().unwrap_or(INSTALLATION_TENANT)
    }

    pub fn allows(
        &self,
        principal_key: &str,
        profile: &str,
        tenant_key: Option<&str>,
        data_labels: &BTreeSet<String>,
    ) -> bool {
        self.principal_key == principal_key
            && self.profile == profile
            && self.tenant_key.as_deref() == tenant_key
            && self.data_labels.is_subset(data_labels)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateTask {
    pub task_id: TaskId,
    pub owner: TaskOwner,
    pub server: String,
    pub task_type: String,
    pub request: Value,
    pub recovery_class: RecoveryClass,
    pub idempotency_key: Option<String>,
    pub ttl_ms: Option<u64>,
    pub poll_interval_ms: Option<u64>,
    pub retention_pins: BTreeSet<TaskRetentionPin>,
}

#[derive(Clone, Debug)]
pub struct CreateTaskResult {
    pub snapshot: TaskSnapshot,
    pub created: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub task_id: TaskId,
    pub owner: TaskOwner,
    pub server: String,
    pub task_type: String,
    pub request: Value,
    pub recovery_class: RecoveryClass,
    pub status: StoreTaskStatus,
    pub status_message: Option<String>,
    pub progress: f64,
    pub result: Option<Value>,
    pub error: Option<TaskFailure>,
    pub idempotency_key: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub cancel_requested_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub retention_expires_at: Option<DateTime<Utc>>,
    pub retention_pins: BTreeSet<TaskRetentionPin>,
    pub ttl_ms: Option<u64>,
    pub poll_interval_ms: Option<u64>,
}

impl TaskSnapshot {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            StoreTaskStatus::Succeeded | StoreTaskStatus::Failed | StoreTaskStatus::Cancelled
        )
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TaskRetentionPin(String);

impl TaskRetentionPin {
    pub fn new(value: impl Into<String>) -> Result<Self, TaskRetentionPinError> {
        let value = value.into();
        if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
            return Err(TaskRetentionPinError);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TaskRetentionPin {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for TaskRetentionPin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("task retention pin is empty, too long, or contains a control character")]
pub struct TaskRetentionPinError;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskFailure {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskInputRequest {
    pub method: String,
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TaskInputExchange {
    pub key: String,
    pub request: TaskInputRequest,
    pub response: Option<BTreeMap<String, Value>>,
    pub created_at: DateTime<Utc>,
    pub responded_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TaskInputSubmission {
    pub accepted: usize,
    pub ignored: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct TaskUpdateCursor(i64);

impl TaskUpdateCursor {
    pub const fn initial() -> Self {
        Self(0)
    }

    pub const fn sequence(self) -> i64 {
        self.0
    }

    pub const fn from_sequence(sequence: i64) -> Option<Self> {
        if sequence < 0 {
            None
        } else {
            Some(Self(sequence))
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TaskUpdate {
    pub cursor: TaskUpdateCursor,
    pub snapshot: TaskSnapshot,
}

impl TaskFailure {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn interrupted_indeterminate() -> Self {
        Self::new(
            "interrupted_indeterminate",
            "execution was interrupted; commit state is indeterminate and the task will not be replayed",
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TaskTransition {
    Running { message: String, progress: f64 },
    Waiting { message: String, progress: f64 },
    Succeeded { message: String, result: Value },
    Failed(TaskFailure),
    CancelRequested,
    Cancelled,
}

impl TaskTransition {
    pub(crate) fn status(&self) -> StoreTaskStatus {
        match self {
            Self::Running { .. } => StoreTaskStatus::Running,
            Self::Waiting { .. } => StoreTaskStatus::Waiting,
            Self::Succeeded { .. } => StoreTaskStatus::Succeeded,
            Self::Failed(_) => StoreTaskStatus::Failed,
            Self::CancelRequested => StoreTaskStatus::CancelRequested,
            Self::Cancelled => StoreTaskStatus::Cancelled,
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::Running { message, .. }
            | Self::Waiting { message, .. }
            | Self::Succeeded { message, .. } => message.clone(),
            Self::Failed(failure) => failure.message.clone(),
            Self::CancelRequested => "cancellation requested".to_owned(),
            Self::Cancelled => "cancelled".to_owned(),
        }
    }

    pub(crate) fn progress(&self, current: f64) -> f64 {
        match self {
            Self::Running { progress, .. } | Self::Waiting { progress, .. } => *progress,
            Self::Succeeded { .. } => 1.0,
            _ => current,
        }
    }

    pub(crate) fn result(&self) -> Option<Value> {
        match self {
            Self::Succeeded { result, .. } => Some(result.clone()),
            _ => None,
        }
    }

    pub(crate) fn failure(&self) -> Option<TaskFailure> {
        match self {
            Self::Failed(failure) => Some(failure.clone()),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TaskPayloadState {
    Completed(Value),
    Failed(TaskFailure),
    Cancelled,
    Running,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct ClaimedTask {
    pub snapshot: TaskSnapshot,
    pub lease_owner: String,
    pub lease_expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default)]
pub struct RecoveryReport {
    pub resumable: Vec<TaskSnapshot>,
    pub webhook_waiting: Vec<TaskSnapshot>,
    pub failed_indeterminate: Vec<TaskSnapshot>,
    pub cancelled: Vec<TaskSnapshot>,
}

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    #[error("task `{0}` was not found")]
    NotFound(String),
    #[error("task `{0}` is not owned by this server")]
    WrongServer(String),
    #[error("task transition from {from:?} to {to:?} is not allowed")]
    InvalidTransition {
        from: StoreTaskStatus,
        to: StoreTaskStatus,
    },
    #[error("task `{0}` changed concurrently")]
    Conflict(String),
    #[error("task `{0}` has an active lease")]
    LeaseHeld(String),
    #[error("task progress must be finite and in 0..=1")]
    InvalidProgress,
    #[error("invalid task invocation authority: {0}")]
    InvalidAuthority(String),
    #[error("invalid persisted task: {0}")]
    InvalidRecord(String),
    #[error("task input key is empty, too long, or contains a control character")]
    InvalidInputKey,
    #[error("task input key `{0}` has already been used")]
    DuplicateInputKey(String),
    #[error(transparent)]
    Store(#[from] veoveo_platform_store::StoreError),
    #[error(transparent)]
    Config(#[from] veoveo_platform_store::StoreConfigError),
    #[error(transparent)]
    Database(#[from] surrealdb::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RequestEnvelope {
    pub input: Value,
    pub owner: TaskOwner,
    pub status_message: Option<String>,
    pub ttl_ms: Option<u64>,
    pub poll_interval_ms: Option<u64>,
}

impl RequestEnvelope {
    pub(crate) fn into_open_object(self) -> Result<OpenObject, serde_json::Error> {
        let Value::Object(values) = serde_json::to_value(self)? else {
            unreachable!("request envelope serializes as an object")
        };
        Ok(OpenObject::new(values.into_iter().collect()))
    }

    pub(crate) fn from_open_object(value: OpenObject) -> Result<Self, serde_json::Error> {
        serde_json::from_value(Value::Object(value.into_map().into_iter().collect()))
    }
}

pub(crate) fn value_to_open_object(value: Value) -> OpenObject {
    match value {
        Value::Object(values) => OpenObject::new(values.into_iter().collect()),
        value => OpenObject::new(BTreeMap::from([("value".to_owned(), value)])),
    }
}

pub(crate) fn open_object_to_value(value: OpenObject) -> Value {
    let mut values: serde_json::Map<String, Value> = value.into_map().into_iter().collect();
    if values.len() == 1 && values.contains_key("value") {
        values.remove("value").unwrap_or(Value::Null)
    } else {
        Value::Object(values)
    }
}

pub(crate) fn failure_to_open_object(failure: &TaskFailure) -> OpenObject {
    value_to_open_object(serde_json::to_value(failure).expect("TaskFailure serializes"))
}

pub(crate) fn record_to_snapshot(record: TaskRecord) -> Result<TaskSnapshot, TaskError> {
    let task_id = task_id_from_record(&record.id)?;
    let envelope = RequestEnvelope::from_open_object(record.request)?;
    let authority = crate::runtime::authority_record(&envelope.owner.authority);
    let initiator = authority
        .initiator_key
        .as_deref()
        .map(|initiator| {
            deterministic_principal_id(envelope.owner.tenant_key(), initiator)
                .map(|principal| principal.record_id())
        })
        .transpose()?;
    if record.tenant != deterministic_tenant_id(envelope.owner.tenant_key())?.record_id()
        || record.owner
            != deterministic_principal_id(
                envelope.owner.tenant_key(),
                &envelope.owner.principal_key,
            )?
            .record_id()
        || record.work_context
            != deterministic_work_context_id(
                envelope.owner.tenant_key(),
                envelope.owner.authority.work_context.as_str(),
            )?
            .record_id()
        || record.initiator != initiator
        || record.invocation_mode != authority.invocation_mode
        || record.delegation_id != authority.delegation_id
        || record.policy_revision != authority.policy_revision
        || record.authority != authority
    {
        return Err(TaskError::InvalidRecord(
            "task owner and invocation authority do not match canonical platform state".to_owned(),
        ));
    }
    let error = record
        .error
        .map(open_object_to_value)
        .map(serde_json::from_value)
        .transpose()?;
    Ok(TaskSnapshot {
        task_id,
        owner: envelope.owner,
        server: record_key(&record.server)?,
        task_type: record.task_type,
        request: envelope.input,
        recovery_class: record.recovery_class.into(),
        status: record.status,
        status_message: envelope.status_message,
        progress: record.progress,
        result: record.result.map(open_object_to_value),
        error,
        idempotency_key: record.idempotency_key,
        lease_owner: record.lease_owner,
        lease_expires_at: record.lease_expires_at,
        cancel_requested_at: record.cancel_requested_at,
        created_at: record.created_at,
        updated_at: record.updated_at,
        started_at: record.started_at,
        completed_at: record.completed_at,
        retention_expires_at: record.retention_expires_at,
        retention_pins: record
            .retention_pins
            .into_iter()
            .map(TaskRetentionPin::new)
            .collect::<Result<_, _>>()
            .map_err(|error| TaskError::InvalidRecord(error.to_string()))?,
        ttl_ms: envelope.ttl_ms,
        poll_interval_ms: envelope.poll_interval_ms,
    })
}

impl TryFrom<TaskRecord> for TaskSnapshot {
    type Error = TaskError;

    fn try_from(record: TaskRecord) -> Result<Self, Self::Error> {
        record_to_snapshot(record)
    }
}

pub(crate) fn task_id_from_record(record: &RecordId) -> Result<TaskId, TaskError> {
    let uuid = match &record.key {
        RecordIdKey::Uuid(value) => uuid::Uuid::parse_str(&value.to_string()),
        RecordIdKey::String(value) => uuid::Uuid::parse_str(value),
        _ => {
            return Err(TaskError::InvalidRecord(format!(
                "task id has non-UUID key: {:?}",
                record.key
            )));
        }
    }
    .map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
    task_id_from_uuid(uuid)
}

pub(crate) fn record_key(record: &RecordId) -> Result<String, TaskError> {
    match &record.key {
        RecordIdKey::String(value) => Ok(value.clone()),
        RecordIdKey::Uuid(value) => Ok(value.to_string()),
        RecordIdKey::Number(value) => Ok(value.to_string()),
        other => Err(TaskError::InvalidRecord(format!(
            "unsupported record key {other:?}"
        ))),
    }
}

pub(crate) fn parse_task_id(value: &str) -> Result<TaskId, TaskError> {
    let task_id =
        TaskId::from_str(value).map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
    task_id_from_uuid(task_id.as_uuid())
}

fn task_id_from_uuid(uuid: uuid::Uuid) -> Result<TaskId, TaskError> {
    if uuid.get_version_num() != 7 {
        return Err(TaskError::InvalidRecord(
            "task id must be a UUIDv7".to_owned(),
        ));
    }
    Ok(TaskId::from_uuid(uuid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_task_ids_must_be_uuid_v7() {
        let version_seven = uuid::Uuid::now_v7().to_string();
        assert!(parse_task_id(&version_seven).is_ok());

        let version_four = uuid::Uuid::new_v4().to_string();
        assert!(matches!(
            parse_task_id(&version_four),
            Err(TaskError::InvalidRecord(message)) if message == "task id must be a UUIDv7"
        ));
    }

    #[test]
    fn task_update_cursors_reject_negative_sequences() {
        assert_eq!(TaskUpdateCursor::from_sequence(-1), None);
        assert_eq!(
            TaskUpdateCursor::from_sequence(42).map(TaskUpdateCursor::sequence),
            Some(42)
        );
    }
}
