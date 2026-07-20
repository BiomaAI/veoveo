use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, RecordIdKey};
use uuid::Uuid;
use veoveo_platform_store::{
    AgentElicitationId, AgentElicitationState, AgentEpisodeId, AgentEpisodeState, AgentTaskId,
    InvocationAuthorityRecord, OpenObject, TaskId, WakeId, WakeKind,
};
use veoveo_task_runtime::TaskRetentionPin;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AgentInstanceId(Uuid);

impl AgentInstanceId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for AgentInstanceId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for AgentInstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for AgentInstanceId {
    type Err = uuid::Error;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

#[derive(Clone, Debug)]
pub struct AgentSpec {
    pub tenant_key: String,
    pub agent_key: String,
    pub display_name: String,
    pub profile: String,
    pub authority: InvocationAuthorityRecord,
    pub manifest: OpenObject,
    pub memory_database: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentLease {
    pub fence: i64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct NewWake {
    pub wake_id: WakeId,
    pub kind: WakeKind,
    pub dedupe_key: Option<String>,
    pub payload: OpenObject,
    pub available_at: DateTime<Utc>,
}

impl NewWake {
    pub fn now(kind: WakeKind, dedupe_key: Option<String>, payload: OpenObject) -> Self {
        Self {
            wake_id: WakeId::new(),
            kind,
            dedupe_key,
            payload,
            available_at: Utc::now(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClaimedWake {
    pub wake_id: WakeId,
    pub kind: WakeKind,
    pub dedupe_key: Option<String>,
    pub payload: OpenObject,
    pub attempts: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpisodeHandle {
    pub episode_id: AgentEpisodeId,
    pub sequence: i64,
    pub retention_pin: TaskRetentionPin,
}

#[derive(Clone, Debug)]
pub struct EpisodeCompletion {
    pub state: AgentEpisodeState,
    pub final_output: String,
    pub summary: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub completion_calls: u64,
    pub tool_calls: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewAgentTask {
    pub task_id: TaskId,
    pub tool_name: String,
    pub descriptor: OpenObject,
    pub descriptor_complete: bool,
    pub retention_pin: TaskRetentionPin,
    pub started_by_episode: AgentEpisodeId,
}

#[derive(Clone, Debug)]
pub struct ClaimedAgentTask {
    pub agent_task_id: AgentTaskId,
    pub task_id: TaskId,
    pub tool_name: String,
    pub descriptor: OpenObject,
    pub descriptor_complete: bool,
    pub attempt_count: i64,
}

#[derive(Clone, Debug)]
pub struct AgentTaskResult {
    pub task_id: TaskId,
    pub tool_name: String,
    pub result: OpenObject,
    pub is_error: bool,
}

#[derive(Clone, Debug)]
pub struct NewElicitation {
    pub elicitation_id: AgentElicitationId,
    pub related_task: Option<TaskId>,
    pub message: String,
    pub requested_schema: Option<OpenObject>,
}

#[derive(Clone, Debug)]
pub struct ParkedElicitation {
    pub elicitation_id: AgentElicitationId,
    pub related_task: Option<TaskId>,
    pub message: String,
    pub requested_schema: Option<OpenObject>,
}

#[derive(Clone, Debug)]
pub struct ElicitationAnswer {
    pub state: AgentElicitationState,
    pub answer: Option<OpenObject>,
    pub answered_by: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AgentRuntimeError {
    #[error("agent runtime requires database-scoped SurrealDB credentials")]
    DatabaseCredentialsRequired,
    #[error("agent `{0}` is already registered with incompatible configuration")]
    AgentConflict(String),
    #[error("this replica does not hold the current agent lease")]
    LeaseLost,
    #[error("record `{entity}` was not found")]
    NotFound { entity: &'static str },
    #[error("record `{entity}` changed concurrently")]
    Conflict { entity: &'static str },
    #[error("invalid {field}: {reason}")]
    InvalidField { field: &'static str, reason: String },
    #[error("numeric value for `{0}` exceeds the platform range")]
    NumericOverflow(&'static str),
    #[error(transparent)]
    Store(#[from] veoveo_platform_store::StoreError),
    #[error(transparent)]
    Database(#[from] surrealdb::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, AgentRuntimeError>;

pub(crate) fn object(entries: impl IntoIterator<Item = (String, serde_json::Value)>) -> OpenObject {
    OpenObject::new(entries.into_iter().collect::<BTreeMap<_, _>>())
}

pub fn json_object(value: serde_json::Value, field: &'static str) -> Result<OpenObject> {
    match value {
        serde_json::Value::Object(values) => Ok(OpenObject::new(
            values.into_iter().collect::<BTreeMap<_, _>>(),
        )),
        _ => Err(AgentRuntimeError::InvalidField {
            field,
            reason: "must be a JSON object".to_owned(),
        }),
    }
}

pub fn wrapped_json(value: serde_json::Value) -> OpenObject {
    object([("value".to_owned(), value)])
}

pub(crate) fn uuid_from_record(record: &RecordId, entity: &'static str) -> Result<Uuid> {
    match &record.key {
        RecordIdKey::Uuid(value) => Uuid::parse_str(&value.to_string()),
        RecordIdKey::String(value) => Uuid::parse_str(value),
        key => {
            return Err(AgentRuntimeError::InvalidField {
                field: entity,
                reason: format!("expected UUID record key, got {key:?}"),
            });
        }
    }
    .map_err(|error| AgentRuntimeError::InvalidField {
        field: entity,
        reason: error.to_string(),
    })
}

pub(crate) fn task_id_from_record(record: &RecordId) -> Result<TaskId> {
    Ok(TaskId::from_uuid(uuid_from_record(record, "task.id")?))
}

pub(crate) fn checked_i64(value: u64, field: &'static str) -> Result<i64> {
    i64::try_from(value).map_err(|_| AgentRuntimeError::NumericOverflow(field))
}

pub const DEFAULT_AGENT_LEASE: Duration = Duration::from_secs(30);
pub const DEFAULT_CLAIM_LEASE: Duration = Duration::from_secs(60);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_and_wake_ids_are_uuid_v7() {
        assert_eq!(AgentInstanceId::new().as_uuid().get_version_num(), 7);
        assert_eq!(
            NewWake::now(WakeKind::Timer, None, object([]))
                .wake_id
                .as_uuid()
                .get_version_num(),
            7
        );
    }

    #[test]
    fn open_boundaries_reject_non_objects() {
        assert!(json_object(serde_json::json!({"ok": true}), "payload").is_ok());
        assert!(json_object(serde_json::json!([1, 2]), "payload").is_err());
    }
}
