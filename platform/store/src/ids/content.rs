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
