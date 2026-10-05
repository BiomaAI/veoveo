use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    AuthorityReleaseId, CalendarId, ClockQualityPolicy, MissionEpoch, OperationalCalendar,
    TimeAcquisitionId, TimeSourceId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdminPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityDatasetKind {
    Tzdb,
    LeapSeconds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityReleaseState {
    Staged,
    Active,
    Retired,
    Quarantined,
}

/// Persisted metadata always carries a positive version.
/// ```compile_fail
/// use veoveo_time_mcp::TimeSource;
/// fn cannot_clear_version(source: &mut TimeSource) { source.record_version = 0; }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeSource {
    pub source_id: TimeSourceId,
    pub name: String,
    pub dataset_kind: AuthorityDatasetKind,
    pub url: String,
    pub expected_content_type: String,
    pub enabled: bool,
    pub record_version: super::TimeVersion,
}

/// Source-creation input retains the published zero-version wire field.
/// ```compile_fail
/// use veoveo_time_mcp::{NewTimeSource, TimeSource};
/// fn create(source: NewTimeSource) {}
/// fn cannot_recreate(existing: TimeSource) { create(existing); }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NewTimeSource {
    pub source_id: TimeSourceId,
    pub name: String,
    pub dataset_kind: AuthorityDatasetKind,
    pub url: String,
    pub expected_content_type: String,
    pub enabled: bool,
    pub record_version: super::SourceCreationVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthorityRelease {
    pub release_id: AuthorityReleaseId,
    pub source_id: TimeSourceId,
    pub dataset_kind: AuthorityDatasetKind,
    pub version_label: String,
    pub source_url: String,
    pub source_digest_sha256: super::AuthoritySourceDigest,
    pub artifact_path: String,
    pub state: AuthorityReleaseState,
    pub retrieved_at: DateTime<Utc>,
    pub validated_at: DateTime<Utc>,
    pub record_version: super::TimeVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimeAcquisitionStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    CancelRequested,
    Cancelled,
}

/// Progress vocabulary emitted by the acquisition worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum TimeAcquisitionPhase {
    #[vocabulary(rename = "queued")]
    Queued,
    #[vocabulary(rename = "downloading")]
    Downloading,
    #[vocabulary(rename = "validating")]
    Validating,
    #[vocabulary(rename = "complete")]
    Complete,
    #[vocabulary(rename = "cancelling")]
    Cancelling,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
    #[vocabulary(rename = "failed")]
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TimeAcquisition {
    pub acquisition_id: TimeAcquisitionId,
    pub source_id: TimeSourceId,
    pub expected_source_digest_sha256: Option<super::AuthoritySourceDigest>,
    pub status: TimeAcquisitionStatus,
    pub phase: TimeAcquisitionPhase,
    pub staged_release_id: Option<AuthorityReleaseId>,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub record_version: super::TimeVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateSourceRequest {
    pub source: NewTimeSource,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReplaceSourceRequest {
    pub source: TimeSource,
    pub expected_record_version: super::TimeVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateAcquisitionRequest {
    pub source_id: TimeSourceId,
    pub expected_source_digest_sha256: Option<super::AuthoritySourceDigest>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ActivateReleaseRequest {
    pub expected_release_record_version: super::TimeVersion,
    pub expected_active_pointer_version: super::TimeWriteGuard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateCalendarRequest {
    pub calendar: OperationalCalendar,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CalendarVersionPath {
    pub calendar_id: CalendarId,
    pub version: super::TimeVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpsertMissionEpochRequest {
    pub epoch: MissionEpoch,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReplaceClockQualityPolicyRequest {
    pub policy: ClockQualityPolicy,
    pub expected_record_version: super::TimeWriteGuard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdminError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub trace_id: String,
}
