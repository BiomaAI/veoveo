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
pub struct ArtifactBlobId(Uuid);
impl ArtifactBlobId {
    pub const TABLE: &'static str = "artifact_blob";
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
impl From<ArtifactBlobId> for Uuid {
    fn from(value: ArtifactBlobId) -> Self {
        value.0
    }
}
impl From<ArtifactBlobId> for RecordId {
    fn from(value: ArtifactBlobId) -> Self {
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
pub struct ArtifactId(Uuid);
impl ArtifactId {
    pub const TABLE: &'static str = "artifact_occurrence";
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
impl From<ArtifactId> for Uuid {
    fn from(value: ArtifactId) -> Self {
        value.0
    }
}
impl From<ArtifactId> for RecordId {
    fn from(value: ArtifactId) -> Self {
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
pub struct ShareLinkId(Uuid);
impl ShareLinkId {
    pub const TABLE: &'static str = "share_link";
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
impl From<ShareLinkId> for Uuid {
    fn from(value: ShareLinkId) -> Self {
        value.0
    }
}
impl From<ShareLinkId> for RecordId {
    fn from(value: ShareLinkId) -> Self {
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
pub struct ArtifactWriteCapabilityId(Uuid);
impl ArtifactWriteCapabilityId {
    pub const TABLE: &'static str = "artifact_write_capability";
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
impl From<ArtifactWriteCapabilityId> for Uuid {
    fn from(value: ArtifactWriteCapabilityId) -> Self {
        value.0
    }
}
impl From<ArtifactWriteCapabilityId> for RecordId {
    fn from(value: ArtifactWriteCapabilityId) -> Self {
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
pub struct ArtifactReadCapabilityId(Uuid);
impl ArtifactReadCapabilityId {
    pub const TABLE: &'static str = "artifact_read_capability";
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
impl From<ArtifactReadCapabilityId> for Uuid {
    fn from(value: ArtifactReadCapabilityId) -> Self {
        value.0
    }
}
impl From<ArtifactReadCapabilityId> for RecordId {
    fn from(value: ArtifactReadCapabilityId) -> Self {
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
pub struct ArtifactWriteRedemptionId(Uuid);
impl ArtifactWriteRedemptionId {
    pub const TABLE: &'static str = "artifact_write_redemption";
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
impl From<ArtifactWriteRedemptionId> for Uuid {
    fn from(value: ArtifactWriteRedemptionId) -> Self {
        value.0
    }
}
impl From<ArtifactWriteRedemptionId> for RecordId {
    fn from(value: ArtifactWriteRedemptionId) -> Self {
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
pub struct ArtifactAccessRequestId(Uuid);
impl ArtifactAccessRequestId {
    pub const TABLE: &'static str = "artifact_access_request";
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
impl From<ArtifactAccessRequestId> for Uuid {
    fn from(value: ArtifactAccessRequestId) -> Self {
        value.0
    }
}
impl From<ArtifactAccessRequestId> for RecordId {
    fn from(value: ArtifactAccessRequestId) -> Self {
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
pub struct MediaTaskContextId(Uuid);
impl MediaTaskContextId {
    pub const TABLE: &'static str = "media_task_context";
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
impl From<MediaTaskContextId> for Uuid {
    fn from(value: MediaTaskContextId) -> Self {
        value.0
    }
}
impl From<MediaTaskContextId> for RecordId {
    fn from(value: MediaTaskContextId) -> Self {
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
pub struct MediaUsageId(Uuid);
impl MediaUsageId {
    pub const TABLE: &'static str = "media_usage";
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
impl From<MediaUsageId> for Uuid {
    fn from(value: MediaUsageId) -> Self {
        value.0
    }
}
impl From<MediaUsageId> for RecordId {
    fn from(value: MediaUsageId) -> Self {
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
pub struct DomainUsageId(Uuid);
impl DomainUsageId {
    pub const TABLE: &'static str = "domain_usage";
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
impl From<DomainUsageId> for Uuid {
    fn from(value: DomainUsageId) -> Self {
        value.0
    }
}
impl From<DomainUsageId> for RecordId {
    fn from(value: DomainUsageId) -> Self {
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
pub struct RecordingDatasetId(Uuid);
impl RecordingDatasetId {
    pub const TABLE: &'static str = "recording_dataset";
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
impl From<RecordingDatasetId> for Uuid {
    fn from(value: RecordingDatasetId) -> Self {
        value.0
    }
}
impl From<RecordingDatasetId> for RecordId {
    fn from(value: RecordingDatasetId) -> Self {
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
pub struct RecordingId(Uuid);
impl RecordingId {
    pub const TABLE: &'static str = "recording";
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
impl From<RecordingId> for Uuid {
    fn from(value: RecordingId) -> Self {
        value.0
    }
}
impl From<RecordingId> for RecordId {
    fn from(value: RecordingId) -> Self {
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
pub struct RecordingLayerId(Uuid);
impl RecordingLayerId {
    pub const TABLE: &'static str = "recording_layer";
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
impl From<RecordingLayerId> for Uuid {
    fn from(value: RecordingLayerId) -> Self {
        value.0
    }
}
impl From<RecordingLayerId> for RecordId {
    fn from(value: RecordingLayerId) -> Self {
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
pub struct RecordingReadGrantId(Uuid);
impl RecordingReadGrantId {
    pub const TABLE: &'static str = "recording_read_grant";
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
impl From<RecordingReadGrantId> for Uuid {
    fn from(value: RecordingReadGrantId) -> Self {
        value.0
    }
}
impl From<RecordingReadGrantId> for RecordId {
    fn from(value: RecordingReadGrantId) -> Self {
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
pub struct RecordingProjectionReceiptId(Uuid);
impl RecordingProjectionReceiptId {
    pub const TABLE: &'static str = "recording_projection_receipt";
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
impl From<RecordingProjectionReceiptId> for Uuid {
    fn from(value: RecordingProjectionReceiptId) -> Self {
        value.0
    }
}
impl From<RecordingProjectionReceiptId> for RecordId {
    fn from(value: RecordingProjectionReceiptId) -> Self {
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
pub struct RecordingIngestStreamId(Uuid);
impl RecordingIngestStreamId {
    pub const TABLE: &'static str = "recording_ingest_stream";
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
impl From<RecordingIngestStreamId> for Uuid {
    fn from(value: RecordingIngestStreamId) -> Self {
        value.0
    }
}
impl From<RecordingIngestStreamId> for RecordId {
    fn from(value: RecordingIngestStreamId) -> Self {
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
pub struct RecordingIngestBatchId(Uuid);
impl RecordingIngestBatchId {
    pub const TABLE: &'static str = "recording_ingest_batch";
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
impl From<RecordingIngestBatchId> for Uuid {
    fn from(value: RecordingIngestBatchId) -> Self {
        value.0
    }
}
impl From<RecordingIngestBatchId> for RecordId {
    fn from(value: RecordingIngestBatchId) -> Self {
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
pub struct RecordingIngestQuotaWindowId(Uuid);
impl RecordingIngestQuotaWindowId {
    pub const TABLE: &'static str = "recording_ingest_quota_window";
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
impl From<RecordingIngestQuotaWindowId> for Uuid {
    fn from(value: RecordingIngestQuotaWindowId) -> Self {
        value.0
    }
}
impl From<RecordingIngestQuotaWindowId> for RecordId {
    fn from(value: RecordingIngestQuotaWindowId) -> Self {
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
pub struct RecordingBlueprintId(Uuid);
impl RecordingBlueprintId {
    pub const TABLE: &'static str = "recording_blueprint";
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
impl From<RecordingBlueprintId> for Uuid {
    fn from(value: RecordingBlueprintId) -> Self {
        value.0
    }
}
impl From<RecordingBlueprintId> for RecordId {
    fn from(value: RecordingBlueprintId) -> Self {
        value.record_id()
    }
}
