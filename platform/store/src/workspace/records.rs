use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{RecordId, SurrealValue};

use crate::{PrincipalId, TenantId, WorkContextId, WorkContextMembershipLevel};

/// An authenticated, policy-evaluated server decision. Never deserialize this
/// from a browser request. The digest fences changes to the evaluated context.
#[derive(Clone, SurrealValue)]
pub struct WorkspaceAuthority {
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub principal: RecordId,
    pub context_digest: String,
    pub membership: WorkContextMembershipLevel,
}

impl WorkspaceAuthority {
    pub fn new(
        tenant: TenantId,
        work_context: WorkContextId,
        principal: PrincipalId,
        context_digest: String,
        membership: WorkContextMembershipLevel,
    ) -> Self {
        Self {
            tenant: tenant.record_id(),
            work_context: work_context.record_id(),
            principal: principal.record_id(),
            context_digest,
            membership,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceChat {
    pub id: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub owner: RecordId,
    pub created_by: Option<RecordId>,
    pub title: String,
    pub initial_title: String,
    pub archived: bool,
    pub members_can_invite: bool,
    pub participation: Option<super::WorkspaceParticipation>,
    pub sequence: i64,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceMember {
    pub id: RecordId,
    pub chat: RecordId,
    pub principal: RecordId,
    pub active: bool,
    pub joined_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[surreal(untagged)]
pub enum WorkspaceInvitationState {
    #[serde(rename = "pending")]
    #[surreal(value = "pending")]
    Pending,
    #[serde(rename = "accepted")]
    #[surreal(value = "accepted")]
    Accepted,
    #[serde(rename = "declined")]
    #[surreal(value = "declined")]
    Declined,
    #[serde(rename = "revoked")]
    #[surreal(value = "revoked")]
    Revoked,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceInvitation {
    pub id: RecordId,
    pub chat: RecordId,
    pub inviter: RecordId,
    pub invitee: RecordId,
    pub state: WorkspaceInvitationState,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceMessage {
    pub id: RecordId,
    pub chat: RecordId,
    pub author: RecordId,
    pub text: String,
    pub reply_to: Option<RecordId>,
    pub reply_context: Option<WorkspaceReplyContext>,
    pub addressed_agents: Option<Vec<RecordId>>,
    pub response_agents: Option<Vec<RecordId>>,
    pub sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceReplyContext {
    pub author_name: String,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[surreal(untagged)]
pub enum WorkspaceEventKind {
    #[serde(rename = "created")]
    #[surreal(value = "created")]
    Created,
    #[serde(rename = "message")]
    #[surreal(value = "message")]
    Message,
    #[serde(rename = "invitation")]
    #[surreal(value = "invitation")]
    Invitation,
    #[serde(rename = "membership")]
    #[surreal(value = "membership")]
    Membership,
    #[serde(rename = "settings")]
    #[surreal(value = "settings")]
    Settings,
    #[serde(rename = "agent")]
    #[surreal(value = "agent")]
    Agent,
    #[serde(rename = "run")]
    #[surreal(value = "run")]
    Run,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceEvent {
    pub chat: RecordId,
    pub sequence: i64,
    pub kind: WorkspaceEventKind,
    pub target: RecordId,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceSnapshot {
    pub chat: WorkspaceChat,
    pub members: Vec<WorkspaceMember>,
    pub messages: Vec<WorkspaceMessage>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceEventPage {
    pub through_sequence: i64,
    pub events: Vec<WorkspaceEvent>,
}
