use super::PersistenceIds;
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_types::id;

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "workspace_chat")]
pub struct WorkspaceChatId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "workspace_agent")]
pub struct WorkspaceAgentId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "workspace_run")]
pub struct WorkspaceRunId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "workspace_operation"
)]
pub struct WorkspaceOperationId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "workspace_member")]
pub struct WorkspaceMemberId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "workspace_message")]
pub struct WorkspaceMessageId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "workspace_invitation"
)]
pub struct WorkspaceInvitationId(Uuid);
