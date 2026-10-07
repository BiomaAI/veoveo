use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    AuthorityReleaseId, CalendarId, ClockQualityPolicy, MissionEpoch, OperationalCalendar,
    TimeAcquisitionId, TimeSourceId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
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
#[schemars(rename = "TimeSource")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct TimeSourceValue {
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
#[schemars(rename = "NewTimeSource")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct NewTimeSourceValue {
    pub source_id: TimeSourceId,
    pub name: String,
    pub dataset_kind: AuthorityDatasetKind,
    pub url: String,
    pub expected_content_type: String,
    pub enabled: bool,
    pub record_version: super::SourceCreationVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "AuthorityRelease")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct AuthorityReleaseValue {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
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
#[schemars(rename = "CreateSourceRequest")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CreateSourceRequestValue {
    pub source: NewTimeSource,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ReplaceSourceRequest {
    pub source: TimeSource,
    pub expected_record_version: super::TimeVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "CreateAcquisitionRequest")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CreateAcquisitionRequestValue {
    pub source_id: TimeSourceId,
    pub expected_source_digest_sha256: Option<super::AuthoritySourceDigest>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ActivateReleaseRequest {
    pub expected_release_record_version: super::TimeVersion,
    pub expected_active_pointer_version: super::TimeWriteGuard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "CreateCalendarRequest")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CreateCalendarRequestValue {
    pub calendar: OperationalCalendar,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct CalendarVersionPath {
    pub calendar_id: CalendarId,
    pub version: super::TimeVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "UpsertMissionEpochRequest")]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct UpsertMissionEpochRequestValue {
    pub epoch: MissionEpoch,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ReplaceClockQualityPolicyRequest {
    pub policy: ClockQualityPolicy,
    pub expected_record_version: super::TimeWriteGuard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct AdminError {
    pub code: AdminErrorCode,
    pub message: String,
    pub retryable: bool,
    pub trace_id: TimeCorrelationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum AdminErrorCode {
    #[vocabulary(rename = "invalid_request")]
    InvalidRequest,
    #[vocabulary(rename = "not_found")]
    NotFound,
    #[vocabulary(rename = "version_conflict")]
    VersionConflict,
    #[vocabulary(rename = "internal_error")]
    InternalError,
}

#[doc(hidden)]
pub struct CorrelationIds;
impl veoveo_types::IdProfile for CorrelationIds {
    type Error = super::admission::TimeValueError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::generated_uuid(
            veoveo_types::UuidGrammar::canonical(&[7]),
            |_, _, _| super::admission::TimeValueError("UUIDv7 correlation identity required"),
        );
}
#[veoveo_types::id(uuid(CorrelationIds), fresh)]
pub struct TimeCorrelationId(uuid::Uuid);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "TimeSourceValue", into = "TimeSourceValue")]
pub struct TimeSource(veoveo_types::Checked<TimeSourceValue>);
impl std::ops::Deref for TimeSource {
    type Target = TimeSourceValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for TimeSource {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "TimeSource".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        TimeSourceValue::json_schema(generator)
    }
}
impl TryFrom<TimeSourceValue> for TimeSource {
    type Error = super::admission::TimeValueError;
    fn try_from(value: TimeSourceValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<TimeSource> for TimeSourceValue {
    fn from(value: TimeSource) -> Self {
        value.0.into_inner()
    }
}
impl TimeSourceValue {
    pub fn build(self) -> Result<TimeSource, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for TimeSourceValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        super::admission::text(&self.name, 256)?;
        super::admission::text(&self.expected_content_type, 128)?;
        super::admission::https(&self.url)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "NewTimeSourceValue", into = "NewTimeSourceValue")]
pub struct NewTimeSource(veoveo_types::Checked<NewTimeSourceValue>);
impl std::ops::Deref for NewTimeSource {
    type Target = NewTimeSourceValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for NewTimeSource {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "NewTimeSource".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        NewTimeSourceValue::json_schema(generator)
    }
}
impl TryFrom<NewTimeSourceValue> for NewTimeSource {
    type Error = super::admission::TimeValueError;
    fn try_from(value: NewTimeSourceValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<NewTimeSource> for NewTimeSourceValue {
    fn from(value: NewTimeSource) -> Self {
        value.0.into_inner()
    }
}
impl NewTimeSourceValue {
    pub fn build(self) -> Result<NewTimeSource, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for NewTimeSourceValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        super::admission::text(&self.name, 256)?;
        super::admission::text(&self.expected_content_type, 128)?;
        super::admission::https(&self.url)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "AuthorityReleaseValue", into = "AuthorityReleaseValue")]
pub struct AuthorityRelease(veoveo_types::Checked<AuthorityReleaseValue>);
impl std::ops::Deref for AuthorityRelease {
    type Target = AuthorityReleaseValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for AuthorityRelease {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AuthorityRelease".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        AuthorityReleaseValue::json_schema(generator)
    }
}
impl TryFrom<AuthorityReleaseValue> for AuthorityRelease {
    type Error = super::admission::TimeValueError;
    fn try_from(value: AuthorityReleaseValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<AuthorityRelease> for AuthorityReleaseValue {
    fn from(value: AuthorityRelease) -> Self {
        value.0.into_inner()
    }
}
impl AuthorityReleaseValue {
    pub fn build(self) -> Result<AuthorityRelease, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for AuthorityReleaseValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        super::admission::text(&self.version_label, 256)?;
        super::admission::https(&self.source_url)?;
        super::admission::absolute_path(&self.artifact_path)?;
        if self.validated_at < self.retrieved_at {
            return Err(super::admission::TimeValueError(
                "validation precedes retrieval",
            ));
        }
        Ok(())
    }
}

impl veoveo_types::Check for TimeAcquisition {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        let phase_matches = match self.status {
            TimeAcquisitionStatus::Queued => self.phase == TimeAcquisitionPhase::Queued,
            TimeAcquisitionStatus::Running => matches!(
                self.phase,
                TimeAcquisitionPhase::Downloading | TimeAcquisitionPhase::Validating
            ),
            TimeAcquisitionStatus::Succeeded => self.phase == TimeAcquisitionPhase::Complete,
            TimeAcquisitionStatus::Failed => self.phase == TimeAcquisitionPhase::Failed,
            TimeAcquisitionStatus::CancelRequested => {
                self.phase == TimeAcquisitionPhase::Cancelling
            }
            TimeAcquisitionStatus::Cancelled => self.phase == TimeAcquisitionPhase::Cancelled,
        };
        if !phase_matches
            || self.staged_release_id.is_some() != (self.status == TimeAcquisitionStatus::Succeeded)
            || self.updated_at < self.created_at
        {
            return Err(super::admission::TimeValueError(
                "inconsistent acquisition lifecycle",
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct AcquisitionWire {
    acquisition_id: TimeAcquisitionId,
    source_id: TimeSourceId,
    expected_source_digest_sha256: Option<super::AuthoritySourceDigest>,
    status: TimeAcquisitionStatus,
    phase: TimeAcquisitionPhase,
    staged_release_id: Option<AuthorityReleaseId>,
    message: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    record_version: super::TimeVersion,
}
impl<'de> Deserialize<'de> for TimeAcquisition {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = AcquisitionWire::deserialize(deserializer)?;
        let result = Self {
            acquisition_id: value.acquisition_id,
            source_id: value.source_id,
            expected_source_digest_sha256: value.expected_source_digest_sha256,
            status: value.status,
            phase: value.phase,
            staged_release_id: value.staged_release_id,
            message: value.message,
            created_at: value.created_at,
            updated_at: value.updated_at,
            record_version: value.record_version,
        };
        veoveo_types::Check::check(&result).map_err(serde::de::Error::custom)?;
        Ok(result)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "CreateSourceRequestValue",
    into = "CreateSourceRequestValue"
)]
pub struct CreateSourceRequest(veoveo_types::Checked<CreateSourceRequestValue>);
impl std::ops::Deref for CreateSourceRequest {
    type Target = CreateSourceRequestValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for CreateSourceRequest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CreateSourceRequest".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        CreateSourceRequestValue::json_schema(generator)
    }
}
impl TryFrom<CreateSourceRequestValue> for CreateSourceRequest {
    type Error = super::admission::TimeValueError;
    fn try_from(value: CreateSourceRequestValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<CreateSourceRequest> for CreateSourceRequestValue {
    fn from(value: CreateSourceRequest) -> Self {
        value.0.into_inner()
    }
}
impl CreateSourceRequestValue {
    pub fn build(self) -> Result<CreateSourceRequest, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for CreateSourceRequestValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        super::admission::text(&self.idempotency_key, 256)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "CreateAcquisitionRequestValue",
    into = "CreateAcquisitionRequestValue"
)]
pub struct CreateAcquisitionRequest(veoveo_types::Checked<CreateAcquisitionRequestValue>);
impl std::ops::Deref for CreateAcquisitionRequest {
    type Target = CreateAcquisitionRequestValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for CreateAcquisitionRequest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CreateAcquisitionRequest".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        CreateAcquisitionRequestValue::json_schema(generator)
    }
}
impl TryFrom<CreateAcquisitionRequestValue> for CreateAcquisitionRequest {
    type Error = super::admission::TimeValueError;
    fn try_from(value: CreateAcquisitionRequestValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<CreateAcquisitionRequest> for CreateAcquisitionRequestValue {
    fn from(value: CreateAcquisitionRequest) -> Self {
        value.0.into_inner()
    }
}
impl CreateAcquisitionRequestValue {
    pub fn build(self) -> Result<CreateAcquisitionRequest, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for CreateAcquisitionRequestValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        super::admission::text(&self.idempotency_key, 256)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "CreateCalendarRequestValue",
    into = "CreateCalendarRequestValue"
)]
pub struct CreateCalendarRequest(veoveo_types::Checked<CreateCalendarRequestValue>);
impl std::ops::Deref for CreateCalendarRequest {
    type Target = CreateCalendarRequestValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for CreateCalendarRequest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CreateCalendarRequest".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        CreateCalendarRequestValue::json_schema(generator)
    }
}
impl TryFrom<CreateCalendarRequestValue> for CreateCalendarRequest {
    type Error = super::admission::TimeValueError;
    fn try_from(value: CreateCalendarRequestValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<CreateCalendarRequest> for CreateCalendarRequestValue {
    fn from(value: CreateCalendarRequest) -> Self {
        value.0.into_inner()
    }
}
impl CreateCalendarRequestValue {
    pub fn build(self) -> Result<CreateCalendarRequest, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for CreateCalendarRequestValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        super::admission::text(&self.idempotency_key, 256)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "UpsertMissionEpochRequestValue",
    into = "UpsertMissionEpochRequestValue"
)]
pub struct UpsertMissionEpochRequest(veoveo_types::Checked<UpsertMissionEpochRequestValue>);
impl std::ops::Deref for UpsertMissionEpochRequest {
    type Target = UpsertMissionEpochRequestValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for UpsertMissionEpochRequest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "UpsertMissionEpochRequest".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        UpsertMissionEpochRequestValue::json_schema(generator)
    }
}
impl TryFrom<UpsertMissionEpochRequestValue> for UpsertMissionEpochRequest {
    type Error = super::admission::TimeValueError;
    fn try_from(value: UpsertMissionEpochRequestValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<UpsertMissionEpochRequest> for UpsertMissionEpochRequestValue {
    fn from(value: UpsertMissionEpochRequest) -> Self {
        value.0.into_inner()
    }
}
impl UpsertMissionEpochRequestValue {
    pub fn build(self) -> Result<UpsertMissionEpochRequest, super::admission::TimeValueError> {
        self.try_into()
    }
}
impl veoveo_types::Check for UpsertMissionEpochRequestValue {
    type Error = super::admission::TimeValueError;
    fn check(&self) -> Result<(), Self::Error> {
        super::admission::text(&self.idempotency_key, 256)?;
        Ok(())
    }
}

#[cfg(test)]
mod acquisition_admission_tests {
    use super::*;
    use veoveo_types::Check;
    #[test]
    fn actual_acquisition_progress_pairs_share_decode_and_persistence_admission() {
        let now = chrono::Utc::now();
        let base = TimeAcquisition {
            acquisition_id: TimeAcquisitionId::parse(
                "time-acquisition-00000000-0000-7000-8000-000000000001",
            )
            .unwrap(),
            source_id: TimeSourceId::parse("time-source-00000000-0000-7000-8000-000000000001")
                .unwrap(),
            expected_source_digest_sha256: None,
            status: TimeAcquisitionStatus::Queued,
            phase: TimeAcquisitionPhase::Queued,
            staged_release_id: None,
            message: "queued".into(),
            created_at: now,
            updated_at: now,
            record_version: super::super::TimeVersion::FIRST,
        };
        for (status, phase) in [
            (TimeAcquisitionStatus::Queued, TimeAcquisitionPhase::Queued),
            (
                TimeAcquisitionStatus::Running,
                TimeAcquisitionPhase::Downloading,
            ),
            (
                TimeAcquisitionStatus::Running,
                TimeAcquisitionPhase::Validating,
            ),
            (
                TimeAcquisitionStatus::Succeeded,
                TimeAcquisitionPhase::Complete,
            ),
            (TimeAcquisitionStatus::Failed, TimeAcquisitionPhase::Failed),
            (
                TimeAcquisitionStatus::CancelRequested,
                TimeAcquisitionPhase::Cancelling,
            ),
            (
                TimeAcquisitionStatus::Cancelled,
                TimeAcquisitionPhase::Cancelled,
            ),
        ] {
            let mut progress = base.clone();
            progress.status = status;
            progress.phase = phase;
            if status == TimeAcquisitionStatus::Succeeded {
                progress.staged_release_id =
                    Some(AuthorityReleaseId::parse("time-release-fixture").unwrap());
            }
            progress.check().unwrap();
            let wire = serde_json::to_value(&progress).unwrap();
            assert_eq!(
                serde_json::from_value::<TimeAcquisition>(wire.clone()).unwrap(),
                progress
            );
            let mut bad = wire;
            bad["phase"] = "unrecognized".into();
            assert!(serde_json::from_value::<TimeAcquisition>(bad).is_err());
        }
        for kind in 0..3 {
            let mut bad = base.clone();
            match kind {
                0 => bad.phase = TimeAcquisitionPhase::Complete,
                1 => {
                    bad.staged_release_id =
                        Some(AuthorityReleaseId::parse("time-release-fixture").unwrap())
                }
                _ => bad.updated_at -= chrono::TimeDelta::seconds(1),
            }
            assert!(bad.check().is_err());
            assert!(
                serde_json::from_value::<TimeAcquisition>(serde_json::to_value(bad).unwrap())
                    .is_err()
            );
        }
    }
}
