use super::PersistenceIds;
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_types::id;

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "enterprise")]
pub struct EnterpriseId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "tenant")]
pub struct TenantId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "principal")]
pub struct PrincipalId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "principal_group")]
pub struct GroupId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "oauth_client")]
pub struct OauthClientId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "mcp_server")]
pub struct McpServerId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "profile")]
pub struct ProfileId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "policy_revision")]
pub struct PolicyRevisionId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "work_context")]
pub struct WorkContextId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "provider_job")]
pub struct ProviderJobId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "provider_event")]
pub struct ProviderEventId(Uuid);
