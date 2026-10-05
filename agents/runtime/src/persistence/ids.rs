use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_platform_store::PersistenceIds;
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

#[cfg(test)]
mod identity_profiles {
    use super::*;
    use surrealdb::types::{RecordId, SurrealValue};
    use veoveo_types::Identity;
    fn check<I>(table: &str, record: RecordId)
    where
        I: Identity<Error = uuid::Error>
            + SurrealValue
            + serde::Serialize
            + serde::de::DeserializeOwned
            + std::fmt::Debug
            + PartialEq,
    {
        let raw = "550E8400E29B41D4A716446655440000";
        let uuid = Uuid::parse_str(raw).unwrap();
        let id = I::parse_identity(raw).unwrap();
        assert_eq!(id.identity_text(), uuid.to_string());
        assert_eq!(record.table.as_str(), table);
        assert_eq!(serde_json::to_value(&id).unwrap(), uuid.to_string());
        let sdk = id.into_value();
        assert_eq!(sdk, uuid.into_value());
        assert_eq!(I::from_value(sdk).unwrap(), I::parse_identity(raw).unwrap());
        assert!(I::parse_identity("invalid").is_err());
    }
    #[test]
    fn owner_table_identities_preserve_uuid_admission_and_sdk_wire() {
        let uuid = Uuid::parse_str("550E8400E29B41D4A716446655440000").unwrap();
        check::<AgentId>("agent", AgentId::from_uuid(uuid).record_id());
        check::<WakeId>("wake", WakeId::from_uuid(uuid).record_id());
        check::<AgentEpisodeId>("agent_episode", AgentEpisodeId::from_uuid(uuid).record_id());
        check::<AgentTaskId>("agent_task", AgentTaskId::from_uuid(uuid).record_id());
        check::<AgentInputRequestId>(
            "agent_input_request",
            AgentInputRequestId::from_uuid(uuid).record_id(),
        );
    }
}
