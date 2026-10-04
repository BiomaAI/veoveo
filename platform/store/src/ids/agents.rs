use super::PersistenceIds;
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_types::id;

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "agent")]
pub struct AgentId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "wake")]
pub struct WakeId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "agent_episode")]
pub struct AgentEpisodeId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "agent_task")]
pub struct AgentTaskId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "agent_input_request"
)]
pub struct AgentInputRequestId(Uuid);
