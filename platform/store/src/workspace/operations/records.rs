use chrono::{DateTime, Utc};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use crate::{WorkspaceChatId, WorkspaceRunId};

/// MCP results and continuation envelopes are opaque to persistence. Only the
/// gateway decodes them, using the pinned SDK. These records are never HTTP DTOs.
#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub struct WorkspaceOperation {
    pub id: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub owner: RecordId,
    pub chat: RecordId,
    pub run: Option<RecordId>,
    /// Read projection from the retained run and agent, independent of chat access.
    pub agent: Option<WorkspaceOperationAgent>,
    pub profile: String,
    pub app_uri: Option<String>,
    pub tool: String,
    pub arguments: String,
    pub phase: WorkspaceOperationPhase,
    pub progress: Option<WorkspaceOperationProgress>,
    pub fence: Uuid,
    pub revision: i64,
    pub round: i64,
    pub dispatch_until: DateTime<Utc>,
    pub task_id: Option<String>,
    pub response: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request-scoped progress, independent of durable Task completion.
#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub struct WorkspaceOperationProgress {
    pub completed: f64,
    pub total: Option<f64>,
    pub message: Option<String>,
}
impl WorkspaceOperationProgress {
    pub fn valid(&self) -> bool {
        self.completed.is_finite()
            && self.completed >= 0.0
            && self
                .total
                .is_none_or(|total| total.is_finite() && total >= 0.0)
            && self
                .message
                .as_ref()
                .is_none_or(|message| message.len() <= 2000 && !message.contains('\0'))
    }
}

#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub struct WorkspaceOperationAgent {
    pub id: RecordId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SurrealValue)]
#[surreal(untagged)]
pub enum WorkspaceOperationPhase {
    #[surreal(value = "dispatching")]
    Dispatching,
    #[surreal(value = "input_required")]
    InputRequired,
    #[surreal(value = "task")]
    Task,
    #[surreal(value = "completed")]
    Completed,
    #[surreal(value = "failed")]
    Failed,
    #[surreal(value = "unconfirmed")]
    Unconfirmed,
}

/// Trusted gateway admission. A model caller must supply its live run fence;
/// a direct human action has no run. Arguments contain one bounded JSON object.
pub struct WorkspaceOperationIntent {
    pub chat: WorkspaceChatId,
    pub run: Option<(WorkspaceRunId, Uuid)>,
    pub profile: String,
    pub app_uri: Option<String>,
    pub tool: String,
    pub arguments: String,
}

/// Exactly one receipt claimant may dispatch. An existing receipt is returned
/// without a lease takeover, even after timeout or restart.
pub struct WorkspaceOperationAdmission {
    pub operation: WorkspaceOperation,
    pub dispatch: bool,
}

pub enum WorkspaceOperationOutcome {
    InputRequired(String),
    Task(String),
    Completed(String),
    /// A definitive MCP error, stored without provider error payloads.
    Failed,
    /// Transport loss cannot establish whether the action happened.
    Unconfirmed,
}
