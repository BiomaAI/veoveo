use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue, Uuid as SurrealUuid};
use uuid::Uuid;

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    SurrealValue,
)]
#[serde(transparent)]
#[id(error=uuid::Error,admit=Uuid::parse_str,generate=Uuid::now_v7)]
pub struct WorkspaceChatId(Uuid);
impl WorkspaceChatId {
    pub const TABLE: &'static str = "workspace_chat";
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
    pub fn record_id(self) -> RecordId {
        RecordId::new(Self::TABLE, SurrealUuid::from(self.0))
    }
}
impl From<WorkspaceChatId> for Uuid {
    fn from(value: WorkspaceChatId) -> Self {
        value.0
    }
}
impl From<WorkspaceChatId> for RecordId {
    fn from(value: WorkspaceChatId) -> Self {
        value.record_id()
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    SurrealValue,
)]
#[serde(transparent)]
#[id(error=uuid::Error,admit=Uuid::parse_str,generate=Uuid::now_v7)]
pub struct WorkspaceAgentId(Uuid);
impl WorkspaceAgentId {
    pub const TABLE: &'static str = "workspace_agent";
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
    pub fn record_id(self) -> RecordId {
        RecordId::new(Self::TABLE, SurrealUuid::from(self.0))
    }
}
impl From<WorkspaceAgentId> for Uuid {
    fn from(value: WorkspaceAgentId) -> Self {
        value.0
    }
}
impl From<WorkspaceAgentId> for RecordId {
    fn from(value: WorkspaceAgentId) -> Self {
        value.record_id()
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    SurrealValue,
)]
#[serde(transparent)]
#[id(error=uuid::Error,admit=Uuid::parse_str,generate=Uuid::now_v7)]
pub struct WorkspaceRunId(Uuid);
impl WorkspaceRunId {
    pub const TABLE: &'static str = "workspace_run";
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
    pub fn record_id(self) -> RecordId {
        RecordId::new(Self::TABLE, SurrealUuid::from(self.0))
    }
}
impl From<WorkspaceRunId> for Uuid {
    fn from(value: WorkspaceRunId) -> Self {
        value.0
    }
}
impl From<WorkspaceRunId> for RecordId {
    fn from(value: WorkspaceRunId) -> Self {
        value.record_id()
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    SurrealValue,
)]
#[serde(transparent)]
#[id(error=uuid::Error,admit=Uuid::parse_str,generate=Uuid::now_v7)]
pub struct WorkspaceOperationId(Uuid);
impl WorkspaceOperationId {
    pub const TABLE: &'static str = "workspace_operation";
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
    pub fn record_id(self) -> RecordId {
        RecordId::new(Self::TABLE, SurrealUuid::from(self.0))
    }
}
impl From<WorkspaceOperationId> for Uuid {
    fn from(value: WorkspaceOperationId) -> Self {
        value.0
    }
}
impl From<WorkspaceOperationId> for RecordId {
    fn from(value: WorkspaceOperationId) -> Self {
        value.record_id()
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    SurrealValue,
)]
#[serde(transparent)]
#[id(error=uuid::Error,admit=Uuid::parse_str,generate=Uuid::now_v7)]
pub struct WorkspaceMemberId(Uuid);
impl WorkspaceMemberId {
    pub const TABLE: &'static str = "workspace_member";
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
    pub fn record_id(self) -> RecordId {
        RecordId::new(Self::TABLE, SurrealUuid::from(self.0))
    }
}
impl From<WorkspaceMemberId> for Uuid {
    fn from(value: WorkspaceMemberId) -> Self {
        value.0
    }
}
impl From<WorkspaceMemberId> for RecordId {
    fn from(value: WorkspaceMemberId) -> Self {
        value.record_id()
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    SurrealValue,
)]
#[serde(transparent)]
#[id(error=uuid::Error,admit=Uuid::parse_str,generate=Uuid::now_v7)]
pub struct WorkspaceMessageId(Uuid);
impl WorkspaceMessageId {
    pub const TABLE: &'static str = "workspace_message";
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
    pub fn record_id(self) -> RecordId {
        RecordId::new(Self::TABLE, SurrealUuid::from(self.0))
    }
}
impl From<WorkspaceMessageId> for Uuid {
    fn from(value: WorkspaceMessageId) -> Self {
        value.0
    }
}
impl From<WorkspaceMessageId> for RecordId {
    fn from(value: WorkspaceMessageId) -> Self {
        value.record_id()
    }
}

#[derive(
    veoveo_types::Id,
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    SurrealValue,
)]
#[serde(transparent)]
#[id(error=uuid::Error,admit=Uuid::parse_str,generate=Uuid::now_v7)]
pub struct WorkspaceInvitationId(Uuid);
impl WorkspaceInvitationId {
    pub const TABLE: &'static str = "workspace_invitation";
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
    pub fn record_id(self) -> RecordId {
        RecordId::new(Self::TABLE, SurrealUuid::from(self.0))
    }
}
impl From<WorkspaceInvitationId> for Uuid {
    fn from(value: WorkspaceInvitationId) -> Self {
        value.0
    }
}
impl From<WorkspaceInvitationId> for RecordId {
    fn from(value: WorkspaceInvitationId) -> Self {
        value.record_id()
    }
}
