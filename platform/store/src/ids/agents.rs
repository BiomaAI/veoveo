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
pub struct AgentId(Uuid);
impl AgentId {
    pub const TABLE: &'static str = "agent";
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
impl From<AgentId> for Uuid {
    fn from(value: AgentId) -> Self {
        value.0
    }
}
impl From<AgentId> for RecordId {
    fn from(value: AgentId) -> Self {
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
pub struct WakeId(Uuid);
impl WakeId {
    pub const TABLE: &'static str = "wake";
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
impl From<WakeId> for Uuid {
    fn from(value: WakeId) -> Self {
        value.0
    }
}
impl From<WakeId> for RecordId {
    fn from(value: WakeId) -> Self {
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
pub struct AgentEpisodeId(Uuid);
impl AgentEpisodeId {
    pub const TABLE: &'static str = "agent_episode";
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
impl From<AgentEpisodeId> for Uuid {
    fn from(value: AgentEpisodeId) -> Self {
        value.0
    }
}
impl From<AgentEpisodeId> for RecordId {
    fn from(value: AgentEpisodeId) -> Self {
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
pub struct AgentTaskId(Uuid);
impl AgentTaskId {
    pub const TABLE: &'static str = "agent_task";
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
impl From<AgentTaskId> for Uuid {
    fn from(value: AgentTaskId) -> Self {
        value.0
    }
}
impl From<AgentTaskId> for RecordId {
    fn from(value: AgentTaskId) -> Self {
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
pub struct AgentInputRequestId(Uuid);
impl AgentInputRequestId {
    pub const TABLE: &'static str = "agent_input_request";
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
impl From<AgentInputRequestId> for Uuid {
    fn from(value: AgentInputRequestId) -> Self {
        value.0
    }
}
impl From<AgentInputRequestId> for RecordId {
    fn from(value: AgentInputRequestId) -> Self {
        value.record_id()
    }
}
