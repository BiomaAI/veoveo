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
pub struct EnterpriseId(Uuid);
impl EnterpriseId {
    pub const TABLE: &'static str = "enterprise";
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
impl From<EnterpriseId> for Uuid {
    fn from(value: EnterpriseId) -> Self {
        value.0
    }
}
impl From<EnterpriseId> for RecordId {
    fn from(value: EnterpriseId) -> Self {
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
pub struct TenantId(Uuid);
impl TenantId {
    pub const TABLE: &'static str = "tenant";
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
impl From<TenantId> for Uuid {
    fn from(value: TenantId) -> Self {
        value.0
    }
}
impl From<TenantId> for RecordId {
    fn from(value: TenantId) -> Self {
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
pub struct PrincipalId(Uuid);
impl PrincipalId {
    pub const TABLE: &'static str = "principal";
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
impl From<PrincipalId> for Uuid {
    fn from(value: PrincipalId) -> Self {
        value.0
    }
}
impl From<PrincipalId> for RecordId {
    fn from(value: PrincipalId) -> Self {
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
pub struct GroupId(Uuid);
impl GroupId {
    pub const TABLE: &'static str = "principal_group";
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
impl From<GroupId> for Uuid {
    fn from(value: GroupId) -> Self {
        value.0
    }
}
impl From<GroupId> for RecordId {
    fn from(value: GroupId) -> Self {
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
pub struct OauthClientId(Uuid);
impl OauthClientId {
    pub const TABLE: &'static str = "oauth_client";
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
impl From<OauthClientId> for Uuid {
    fn from(value: OauthClientId) -> Self {
        value.0
    }
}
impl From<OauthClientId> for RecordId {
    fn from(value: OauthClientId) -> Self {
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
pub struct McpServerId(Uuid);
impl McpServerId {
    pub const TABLE: &'static str = "mcp_server";
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
impl From<McpServerId> for Uuid {
    fn from(value: McpServerId) -> Self {
        value.0
    }
}
impl From<McpServerId> for RecordId {
    fn from(value: McpServerId) -> Self {
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
pub struct ProfileId(Uuid);
impl ProfileId {
    pub const TABLE: &'static str = "profile";
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
impl From<ProfileId> for Uuid {
    fn from(value: ProfileId) -> Self {
        value.0
    }
}
impl From<ProfileId> for RecordId {
    fn from(value: ProfileId) -> Self {
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
pub struct PolicyRevisionId(Uuid);
impl PolicyRevisionId {
    pub const TABLE: &'static str = "policy_revision";
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
impl From<PolicyRevisionId> for Uuid {
    fn from(value: PolicyRevisionId) -> Self {
        value.0
    }
}
impl From<PolicyRevisionId> for RecordId {
    fn from(value: PolicyRevisionId) -> Self {
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
pub struct WorkContextId(Uuid);
impl WorkContextId {
    pub const TABLE: &'static str = "work_context";
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
impl From<WorkContextId> for Uuid {
    fn from(value: WorkContextId) -> Self {
        value.0
    }
}
impl From<WorkContextId> for RecordId {
    fn from(value: WorkContextId) -> Self {
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
pub struct ProviderJobId(Uuid);
impl ProviderJobId {
    pub const TABLE: &'static str = "provider_job";
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
impl From<ProviderJobId> for Uuid {
    fn from(value: ProviderJobId) -> Self {
        value.0
    }
}
impl From<ProviderJobId> for RecordId {
    fn from(value: ProviderJobId) -> Self {
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
pub struct ProviderEventId(Uuid);
impl ProviderEventId {
    pub const TABLE: &'static str = "provider_event";
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
impl From<ProviderEventId> for Uuid {
    fn from(value: ProviderEventId) -> Self {
        value.0
    }
}
impl From<ProviderEventId> for RecordId {
    fn from(value: ProviderEventId) -> Self {
        value.record_id()
    }
}
