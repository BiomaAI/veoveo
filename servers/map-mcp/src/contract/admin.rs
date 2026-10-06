use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    AcquisitionId, DatasetRelease, DatasetReleaseId, MapDatasetId, MapSourceId, MobilityProfile,
    RegisteredSource, Wgs84BoundingBox,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateSourceRequest {
    pub source: RegisteredSource,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReplaceSourceRequest {
    pub source: RegisteredSource,
    pub expected_record_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisableSourceRequest {
    pub source_id: MapSourceId,
    pub expected_record_version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateAcquisitionRequest {
    pub source_id: MapSourceId,
    pub requested_coverage: Wgs84BoundingBox,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source_digest_sha256: Option<String>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CancelAcquisitionRequest {
    pub acquisition_id: AcquisitionId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[schemars(rename = "AcquisitionJob")]
pub struct AcquisitionJobValue {
    pub acquisition_id: AcquisitionId,
    pub source_id: MapSourceId,
    pub requested_coverage: Wgs84BoundingBox,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source_digest_sha256: Option<String>,
    pub status: AcquisitionStatus,
    pub progress: AcquisitionProgress,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_artifact_uri: Option<veoveo_artifact_contract::ArtifactUri>,
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
#[serde(deny_unknown_fields)]
pub struct ListActiveDatasetReleasesRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<MapSourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<MapDatasetId>,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ActiveDatasetRelease {
    pub pointer: ActiveReleasePointer,
    pub release: DatasetRelease,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListActiveDatasetReleasesOutput {
    pub releases: Vec<ActiveDatasetRelease>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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

/// Mutable lifecycle state is rechecked on decoding, construction and every serialization.
#[derive(Debug, Clone, PartialEq)]
pub struct AcquisitionJob(AcquisitionJobValue);
impl AcquisitionJob {
    pub fn new(value: AcquisitionJobValue) -> Result<Self, super::MapRelationshipError> {
        veoveo_types::Check::check(&value)?;
        Ok(Self(value))
    }
    pub fn into_value(self) -> AcquisitionJobValue {
        self.0
    }
}
impl std::ops::Deref for AcquisitionJob {
    type Target = AcquisitionJobValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for AcquisitionJob {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl veoveo_types::Check for AcquisitionJobValue {
    type Error = super::MapRelationshipError;
    fn check(&self) -> Result<(), Self::Error> {
        super::relationships::check_acquisition(self)
    }
}
impl Serialize for AcquisitionJob {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        veoveo_types::Check::check(&self.0).map_err(serde::ser::Error::custom)?;
        self.0.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for AcquisitionJob {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(AcquisitionJobValue::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
impl JsonSchema for AcquisitionJob {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        AcquisitionJobValue::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        AcquisitionJobValue::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        AcquisitionJobValue::json_schema(generator)
    }
}
