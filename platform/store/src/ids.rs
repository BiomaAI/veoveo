//! Database table identities, independent of their public domain projections.
mod access;
pub use access::*;
mod content;
pub use content::*;

#[doc(hidden)]
pub struct PersistenceIds;
impl veoveo_types::IdProfile for PersistenceIds {
    type Error = uuid::Error;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> = veoveo_types::IdProfileSpec {
        wire: veoveo_types::IdWire::InnerUuid,
        schema: veoveo_types::IdSchema::DerivedUuid,
        ..veoveo_types::IdProfileSpec::generated_uuid(
            veoveo_types::UuidGrammar {
                versions: &[],
                variant: veoveo_types::UuidVariant::Any,
                spelling: veoveo_types::UuidSpelling::ParserAliases,
            },
            persistence_id_error,
        )
    };
}
fn persistence_id_error(
    value: &str,
    _: veoveo_types::IdMetadata,
    _: veoveo_types::IdFailure,
) -> uuid::Error {
    uuid::Uuid::parse_str(value)
        .expect_err("only malformed UUIDs reach the unrestricted persistence error mapping")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_uuid_v7_and_table_scoped() {
        let id = ArtifactId::new();
        assert_eq!(id.as_uuid().get_version_num(), 7);
        let record = id.record_id();
        assert_eq!(record.table.as_str(), ArtifactId::TABLE);
    }

    #[test]
    fn ids_round_trip_through_json() {
        let id = ArtifactId::new();
        let encoded = serde_json::to_string(&id).unwrap();
        let decoded: ArtifactId = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, id);
    }
}

#[cfg(test)]
mod identity_profiles {
    use super::*;
    use surrealdb::types::{RecordId, SurrealValue};
    use uuid::Uuid;
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
    fn all_table_identities_keep_uuid_admission_and_sdk_mapping() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        check::<EnterpriseId>("enterprise", EnterpriseId::from_uuid(uuid).record_id());
        check::<TenantId>("tenant", TenantId::from_uuid(uuid).record_id());
        check::<PrincipalId>("principal", PrincipalId::from_uuid(uuid).record_id());
        check::<GroupId>("principal_group", GroupId::from_uuid(uuid).record_id());
        check::<OauthClientId>("oauth_client", OauthClientId::from_uuid(uuid).record_id());
        check::<McpServerId>("mcp_server", McpServerId::from_uuid(uuid).record_id());
        check::<ProfileId>("profile", ProfileId::from_uuid(uuid).record_id());
        check::<PolicyRevisionId>(
            "policy_revision",
            PolicyRevisionId::from_uuid(uuid).record_id(),
        );
        check::<WorkContextId>("work_context", WorkContextId::from_uuid(uuid).record_id());
        check::<ProviderJobId>("provider_job", ProviderJobId::from_uuid(uuid).record_id());
        check::<ProviderEventId>(
            "provider_event",
            ProviderEventId::from_uuid(uuid).record_id(),
        );
        check::<ArtifactBlobId>("artifact_blob", ArtifactBlobId::from_uuid(uuid).record_id());
        check::<ArtifactId>(
            "artifact_occurrence",
            ArtifactId::from_uuid(uuid).record_id(),
        );
        check::<ShareLinkId>("share_link", ShareLinkId::from_uuid(uuid).record_id());
        check::<ArtifactWriteCapabilityId>(
            "artifact_write_capability",
            ArtifactWriteCapabilityId::from_uuid(uuid).record_id(),
        );
        check::<ArtifactReadCapabilityId>(
            "artifact_read_capability",
            ArtifactReadCapabilityId::from_uuid(uuid).record_id(),
        );
        check::<ArtifactWriteRedemptionId>(
            "artifact_write_redemption",
            ArtifactWriteRedemptionId::from_uuid(uuid).record_id(),
        );
        check::<ArtifactAccessRequestId>(
            "artifact_access_request",
            ArtifactAccessRequestId::from_uuid(uuid).record_id(),
        );
        check::<MediaTaskContextId>(
            "media_task_context",
            MediaTaskContextId::from_uuid(uuid).record_id(),
        );
        check::<MediaUsageId>("media_usage", MediaUsageId::from_uuid(uuid).record_id());
        check::<DomainUsageId>("domain_usage", DomainUsageId::from_uuid(uuid).record_id());
    }
}

#[cfg(test)]
mod declaration_fixture {
    use surrealdb::types::{RecordId, SurrealValue};
    use uuid::Uuid;
    #[veoveo_types::id(
        uuid(super::PersistenceIds),
        fresh,
        const_uuid,
        surreal = "fixture_identity"
    )]
    struct IndependentId(Uuid);
    #[test]
    fn independent_storage_identity_preserves_uuid_sdk_and_table_admission() {
        const ID: IndependentId = IndependentId::from_uuid(Uuid::nil());
        const UUID: Uuid = ID.as_uuid();
        assert_eq!(UUID, Uuid::nil());
        let id = IndependentId::new();
        assert_eq!(id.as_uuid().get_version_num(), 7);
        let record: RecordId = id.into();
        assert_eq!(record.table.as_str(), IndependentId::TABLE);
        let uuid = Uuid::parse_str("550E8400E29B41D4A716446655440000").unwrap();
        let admitted: IndependentId = "550E8400E29B41D4A716446655440000".parse().unwrap();
        assert_eq!(admitted, IndependentId::from_uuid(uuid));
        assert_eq!(admitted.into_value(), uuid.into_value());
        assert_eq!(
            IndependentId::from_value(uuid.into_value()).unwrap(),
            admitted
        );
        assert_eq!(
            serde_json::from_str::<IndependentId>(&serde_json::to_string(&admitted).unwrap())
                .unwrap(),
            admitted
        );
    }
}
