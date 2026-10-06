use crate::persistence::{AgentTaskOutcome, DeferredTaskDescriptor, WakePayload};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_platform_store::*;

pub use crate::contract::AgentState;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum WakeKind {
    #[vocabulary(rename = "task_result")]
    TaskResult,
    #[vocabulary(rename = "resource_changed")]
    ResourceChanged,
    #[vocabulary(rename = "timer")]
    Timer,
    #[vocabulary(rename = "operator_message")]
    OperatorMessage,
    #[vocabulary(rename = "input_request")]
    InputRequest,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum WakeState {
    #[vocabulary(rename = "pending")]
    Pending,
    #[vocabulary(rename = "claimed")]
    Claimed,
    #[vocabulary(rename = "acked")]
    Acked,
    #[vocabulary(rename = "coalesced")]
    Coalesced,
    #[vocabulary(rename = "failed")]
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum AgentEpisodeState {
    #[vocabulary(rename = "running")]
    Running,
    #[vocabulary(rename = "completed")]
    Completed,
    #[vocabulary(rename = "budget_terminated")]
    BudgetTerminated,
    #[vocabulary(rename = "stopped")]
    Stopped,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "crashed")]
    Crashed,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum AgentTaskWatchState {
    #[vocabulary(rename = "pending")]
    Pending,
    #[vocabulary(rename = "watching")]
    Watching,
    #[vocabulary(rename = "resolved")]
    Resolved,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum AgentInputRequestState {
    #[vocabulary(rename = "pending")]
    Pending,
    #[vocabulary(rename = "answered")]
    Answered,
    #[vocabulary(rename = "declined")]
    Declined,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub agent_key: String,
    pub display_name: String,
    pub profile: RecordId,
    pub work_context: RecordId,
    pub policy_revision: String,
    pub authority: InvocationAuthorityRecord,
    pub managed_ready: Option<crate::persistence::repository::instances::ManagedKernelReady>,
    pub state: AgentState,
    pub manifest: OpenObject,
    pub memory_database: String,
    pub last_episode: Option<RecordId>,
    pub next_episode_sequence: i64,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub fence: i64,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WakeRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub agent: RecordId,
    pub kind: WakeKind,
    pub state: WakeState,
    pub dedupe_key: Option<String>,
    pub payload: WakePayload,
    pub available_at: DateTime<Utc>,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    pub claim_fence: Option<i64>,
    pub attempts: i64,
    pub acked_at: Option<DateTime<Utc>>,
    pub acked_by_episode: Option<RecordId>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: i64,
    pub coalesced_into: Option<RecordId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentEpisodeRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub agent: RecordId,
    pub sequence: i64,
    pub retention_pin: String,
    pub wake_note: String,
    pub managed: Option<crate::persistence::repository::instances::ManagedEpisodeBinding>,
    pub state: AgentEpisodeState,
    pub final_output: Option<String>,
    pub summary: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub completion_calls: i64,
    pub tool_calls: i64,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentTaskRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub agent: RecordId,
    pub task_id: String,
    pub tool_name: String,
    pub descriptor: DeferredTaskDescriptor,
    pub descriptor_complete: bool,
    pub state: AgentTaskWatchState,
    pub result: Option<AgentTaskOutcome>,
    pub result_is_error: bool,
    pub result_wake: Option<RecordId>,
    pub retention_pin: String,
    pub retention_pin_active: bool,
    pub attempt_count: i64,
    pub next_retry_at: DateTime<Utc>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub started_by_episode: RecordId,
    pub consumed_by_episode: Option<RecordId>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentInputRequestRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub agent: RecordId,
    pub related_task: Option<String>,
    pub message: String,
    pub requested_schema: Option<OpenObject>,
    pub state: AgentInputRequestState,
    pub answer: Option<OpenObject>,
    pub answered_by: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
    pub revision: i64,
}

#[cfg(test)]
#[path = "vocabulary_baseline.rs"]
mod vocabulary_baseline;
