use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    AcquisitionId, DatasetRelease, DatasetReleaseId, MapDatasetId, MapSourceId, MobilityProfile,
    RegisteredSource, Wgs84BoundingBox,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateSourceRequest {
    pub source: RegisteredSource,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReplaceSourceRequest {
    pub source: RegisteredSource,
    pub expected_record_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DisableSourceRequest {
    pub source_id: MapSourceId,
    pub expected_record_version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateAcquisitionRequest {
    pub source_id: MapSourceId,
    pub requested_coverage: Wgs84BoundingBox,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source_digest_sha256: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CancelAcquisitionRequest {
    pub acquisition_id: AcquisitionId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateMobilityProfileRequest {
    pub profile: MobilityProfile,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    CancelRequested,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AcquisitionPhase {
    Queued,
    Downloading,
    Verifying,
    Normalizing,
    BuildingGraph,
    Validating,
    PublishingArtifacts,
    StagingRelease,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AcquisitionProgress {
    pub phase: AcquisitionPhase,
    pub completed_units: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_units: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AcquisitionJob {
    pub acquisition_id: AcquisitionId,
    pub source_id: MapSourceId,
    pub requested_coverage: Wgs84BoundingBox,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source_digest_sha256: Option<String>,
    pub status: AcquisitionStatus,
    pub progress: AcquisitionProgress,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_artifact_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub staged_release_id: Option<DatasetReleaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostics_uri: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub record_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ActiveReleasePointer {
    pub dataset_id: MapDatasetId,
    pub release_id: DatasetReleaseId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_release_id: Option<DatasetReleaseId>,
    pub record_version: u64,
    pub activated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseMutationRequest {
    pub release_id: DatasetReleaseId,
    pub expected_record_version: u64,
    pub expected_active_pointer_version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReleaseMutationResponse {
    pub release: DatasetRelease,
    pub invalidated_route_count: u64,
}
