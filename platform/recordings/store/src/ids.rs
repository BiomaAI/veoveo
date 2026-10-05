use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_platform_store::PersistenceIds;
use veoveo_types::id;

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "recording_dataset")]
pub struct RecordingDatasetId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "recording")]
pub struct RecordingId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "recording_layer")]
pub struct RecordingLayerId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "recording_read_grant"
)]
pub struct RecordingReadGrantId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "recording_projection_receipt"
)]
pub struct RecordingProjectionReceiptId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "recording_ingest_stream"
)]
pub struct RecordingIngestStreamId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "recording_ingest_batch"
)]
pub struct RecordingIngestBatchId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "recording_ingest_quota_window"
)]
pub struct RecordingIngestQuotaWindowId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "recording_blueprint"
)]
pub struct RecordingBlueprintId(Uuid);

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
        check::<RecordingDatasetId>(
            "recording_dataset",
            RecordingDatasetId::from_uuid(uuid).record_id(),
        );
        check::<RecordingId>("recording", RecordingId::from_uuid(uuid).record_id());
        check::<RecordingLayerId>(
            "recording_layer",
            RecordingLayerId::from_uuid(uuid).record_id(),
        );
        check::<RecordingReadGrantId>(
            "recording_read_grant",
            RecordingReadGrantId::from_uuid(uuid).record_id(),
        );
        check::<RecordingProjectionReceiptId>(
            "recording_projection_receipt",
            RecordingProjectionReceiptId::from_uuid(uuid).record_id(),
        );
        check::<RecordingIngestStreamId>(
            "recording_ingest_stream",
            RecordingIngestStreamId::from_uuid(uuid).record_id(),
        );
        check::<RecordingIngestBatchId>(
            "recording_ingest_batch",
            RecordingIngestBatchId::from_uuid(uuid).record_id(),
        );
        check::<RecordingIngestQuotaWindowId>(
            "recording_ingest_quota_window",
            RecordingIngestQuotaWindowId::from_uuid(uuid).record_id(),
        );
        check::<RecordingBlueprintId>(
            "recording_blueprint",
            RecordingBlueprintId::from_uuid(uuid).record_id(),
        );
    }
}
