use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceAgent {
    pub id: RecordId,
    pub chat: RecordId,
    pub definition: String,
    pub definition_digest: String,
    pub display_name: String,
    pub provider: String,
    pub model: String,
    pub active: bool,
    pub joined_at: DateTime<Utc>,
}

/// Server-validated catalog admission, never a browser-submitted model config.
#[derive(Clone, SurrealValue)]
pub struct WorkspaceAgentAdmission {
    pub definition: String,
    pub definition_digest: String,
    pub display_name: String,
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum WorkspaceRunState {
    #[surreal(value = "queued")]
    Queued,
    #[surreal(value = "running")]
    Running,
    #[surreal(value = "completed")]
    Completed,
    #[surreal(value = "cancelled")]
    Cancelled,
    #[surreal(value = "interrupted")]
    Interrupted,
    #[surreal(value = "failed")]
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum WorkspaceRunFailure {
    #[surreal(value = "model_unavailable")]
    ModelUnavailable,
    #[surreal(value = "permission_changed")]
    PermissionChanged,
    #[surreal(value = "output_limit")]
    OutputLimit,
    #[surreal(value = "deadline")]
    Deadline,
    #[surreal(value = "worker_lost")]
    WorkerLost,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceRun {
    pub id: RecordId,
    pub chat: RecordId,
    pub agent: RecordId,
    pub definition_digest: String,
    pub initiator: RecordId,
    pub trigger: RecordId,
    pub context_sequence: i64,
    pub sequence: i64,
    pub updated_sequence: i64,
    pub state: WorkspaceRunState,
    pub text: String,
    pub failure: Option<WorkspaceRunFailure>,
    pub fence: Option<Uuid>,
    pub lease_until: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Provider-independent, irreversible publication/heartbeat request. The fence
/// comes from the successful claim, never from a public browser request.
#[derive(Clone, SurrealValue)]
pub struct WorkspaceRunUpdate {
    pub fence: Uuid,
    pub text: String,
    pub state: WorkspaceRunState,
    pub failure: Option<WorkspaceRunFailure>,
}

/// Immutable prompt boundary. Human messages are immutable; only agent results
/// completed before admission are eligible. In-flight and later output is absent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceRunContext {
    pub run: WorkspaceRun,
    pub messages: Vec<super::super::WorkspaceMessage>,
    pub completed_runs: Vec<WorkspaceRun>,
    pub agents: Vec<WorkspaceAgent>,
}
