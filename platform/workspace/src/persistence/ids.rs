use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_platform_store::PersistenceIds;
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
        check::<WorkspaceChatId>(
            "workspace_chat",
            WorkspaceChatId::from_uuid(uuid).record_id(),
        );
        check::<WorkspaceAgentId>(
            "workspace_agent",
            WorkspaceAgentId::from_uuid(uuid).record_id(),
        );
        check::<WorkspaceRunId>("workspace_run", WorkspaceRunId::from_uuid(uuid).record_id());
        check::<WorkspaceOperationId>(
            "workspace_operation",
            WorkspaceOperationId::from_uuid(uuid).record_id(),
        );
        check::<WorkspaceMemberId>(
            "workspace_member",
            WorkspaceMemberId::from_uuid(uuid).record_id(),
        );
        check::<WorkspaceMessageId>(
            "workspace_message",
            WorkspaceMessageId::from_uuid(uuid).record_id(),
        );
        check::<WorkspaceInvitationId>(
            "workspace_invitation",
            WorkspaceInvitationId::from_uuid(uuid).record_id(),
        );
    }
}
