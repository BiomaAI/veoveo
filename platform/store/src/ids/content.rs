use super::PersistenceIds;
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_types::id;

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "artifact_blob")]
pub struct ArtifactBlobId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "artifact_occurrence"
)]
pub struct ArtifactId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "share_link")]
pub struct ShareLinkId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "artifact_write_capability"
)]
pub struct ArtifactWriteCapabilityId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "artifact_read_capability"
)]
pub struct ArtifactReadCapabilityId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "artifact_write_redemption"
)]
pub struct ArtifactWriteRedemptionId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "artifact_access_request"
)]
pub struct ArtifactAccessRequestId(Uuid);

#[id(
    uuid(PersistenceIds),
    fresh,
    const_uuid,
    surreal = "media_task_context"
)]
pub struct MediaTaskContextId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "media_usage")]
pub struct MediaUsageId(Uuid);

#[id(uuid(PersistenceIds), fresh, const_uuid, surreal = "domain_usage")]
pub struct DomainUsageId(Uuid);

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
