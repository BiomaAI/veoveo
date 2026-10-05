//! Private SurrealDB driver records; textual fields are decoded by the Time owner.

use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub(crate) enum TimeDatasetKind {
    #[vocabulary(rename = "tzdb")]
    Tzdb,
    #[vocabulary(rename = "leap_seconds")]
    LeapSeconds,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub(crate) enum TimeAuthorityReleaseState {
    #[vocabulary(rename = "staged")]
    Staged,
    #[vocabulary(rename = "active")]
    Active,
    #[vocabulary(rename = "retired")]
    Retired,
    #[vocabulary(rename = "quarantined")]
    Quarantined,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub(crate) enum TimeAcquisitionState {
    #[vocabulary(rename = "queued")]
    Queued,
    #[vocabulary(rename = "running")]
    Running,
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancel_requested")]
    CancelRequested,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub(crate) enum TimeCalendarState {
    #[vocabulary(rename = "staged")]
    Staged,
    #[vocabulary(rename = "active")]
    Active,
    #[vocabulary(rename = "retired")]
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub(crate) enum TimeTemporalEventState {
    #[vocabulary(rename = "scheduled")]
    Scheduled,
    #[vocabulary(rename = "due")]
    Due,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct TimeSourceRecord {
    pub(crate) id: RecordId,
    pub(crate) tenant: RecordId,
    pub(crate) owner: RecordId,
    pub(crate) source_key: String,
    pub(crate) name: String,
    pub(crate) dataset_kind: TimeDatasetKind,
    pub(crate) source_url: String,
    pub(crate) expected_content_type: String,
    pub(crate) enabled: bool,
    pub(crate) canonical_json: String,
    pub(crate) record_version: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct TimeAuthorityReleaseRecord {
    pub(crate) provenance: TimeProvenanceRecord,
    pub(crate) id: RecordId,
    pub(crate) tenant: RecordId,
    pub(crate) owner: RecordId,
    pub(crate) release_key: String,
    pub(crate) source_key: String,
    pub(crate) dataset_kind: TimeDatasetKind,
    pub(crate) state: TimeAuthorityReleaseState,
    pub(crate) version_label: String,
    pub(crate) source_url: String,
    pub(crate) source_digest_sha256: String,
    pub(crate) artifact_path: String,
    pub(crate) retrieved_at: DateTime<Utc>,
    pub(crate) validated_at: DateTime<Utc>,
    pub(crate) canonical_json: String,
    pub(crate) record_version: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct TimeActiveAuthorityRecord {
    pub(crate) id: RecordId,
    pub(crate) tenant: RecordId,
    pub(crate) dataset_kind: TimeDatasetKind,
    pub(crate) release_key: String,
    pub(crate) previous_release_key: Option<String>,
    pub(crate) activated_by: RecordId,
    pub(crate) activated_at: DateTime<Utc>,
    pub(crate) record_version: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct TimeAcquisitionRecord {
    pub(crate) id: RecordId,
    pub(crate) tenant: RecordId,
    pub(crate) owner: RecordId,
    pub(crate) acquisition_key: String,
    pub(crate) source_key: String,
    pub(crate) expected_source_digest_sha256: Option<String>,
    pub(crate) idempotency_key: String,
    pub(crate) status: TimeAcquisitionState,
    #[surreal(wrap)]
    pub(crate) phase: crate::TimeAcquisitionPhase,
    pub(crate) staged_release_key: Option<String>,
    pub(crate) canonical_json: String,
    pub(crate) record_version: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct TimeCalendarVersionRecord {
    pub(crate) provenance: TimeProvenanceRecord,
    pub(crate) id: RecordId,
    pub(crate) tenant: RecordId,
    pub(crate) owner: RecordId,
    pub(crate) calendar_key: String,
    pub(crate) calendar_version: i64,
    pub(crate) name: String,
    pub(crate) zone_id: String,
    pub(crate) state: TimeCalendarState,
    pub(crate) canonical_json: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct TimeMissionEpochRecord {
    pub(crate) provenance: TimeProvenanceRecord,
    pub(crate) id: RecordId,
    pub(crate) tenant: RecordId,
    pub(crate) owner: RecordId,
    pub(crate) epoch_key: String,
    pub(crate) name: String,
    pub(crate) epoch_version: i64,
    pub(crate) tai_seconds_since_1970: i64,
    pub(crate) nanosecond: i64,
    pub(crate) canonical_json: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct TimeTemporalEventRecord {
    pub(crate) provenance: TimeProvenanceRecord,
    pub(crate) id: RecordId,
    pub(crate) tenant: RecordId,
    pub(crate) owner: RecordId,
    pub(crate) event_key: String,
    pub(crate) name: String,
    pub(crate) state: TimeTemporalEventState,
    pub(crate) due_tai_seconds_since_1970: i64,
    pub(crate) due_nanosecond: i64,
    pub(crate) idempotency_key: String,
    pub(crate) canonical_json: String,
    pub(crate) record_version: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub(crate) struct TimeClockPolicyRecord {
    pub(crate) id: RecordId,
    pub(crate) tenant: RecordId,
    pub(crate) owner: RecordId,
    pub(crate) maximum_error_nanoseconds: i64,
    pub(crate) maximum_stratum: i64,
    pub(crate) minimum_source_diversity: i64,
    pub(crate) maximum_holdover_seconds: i64,
    pub(crate) record_version: i64,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub(super) struct TimeSourceContent {
    pub(super) tenant: RecordId,
    pub(super) owner: RecordId,
    pub(super) source_key: String,
    pub(super) name: String,
    pub(super) dataset_kind: TimeDatasetKind,
    pub(super) source_url: String,
    pub(super) expected_content_type: String,
    pub(super) enabled: bool,
    pub(super) canonical_json: String,
    pub(super) record_version: i64,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub(super) struct TimeAuthorityReleaseContent {
    pub(super) provenance: TimeProvenanceRecord,
    pub(super) tenant: RecordId,
    pub(super) owner: RecordId,
    pub(super) release_key: String,
    pub(super) source_key: String,
    pub(super) dataset_kind: TimeDatasetKind,
    pub(super) state: TimeAuthorityReleaseState,
    pub(super) version_label: String,
    pub(super) source_url: String,
    pub(super) source_digest_sha256: String,
    pub(super) artifact_path: String,
    pub(super) retrieved_at: DateTime<Utc>,
    pub(super) validated_at: DateTime<Utc>,
    pub(super) canonical_json: String,
    pub(super) record_version: i64,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub(super) struct TimeAcquisitionContent {
    pub(super) tenant: RecordId,
    pub(super) owner: RecordId,
    pub(super) acquisition_key: String,
    pub(super) source_key: String,
    pub(super) expected_source_digest_sha256: Option<String>,
    pub(super) idempotency_key: String,
    pub(super) status: TimeAcquisitionState,
    #[surreal(wrap)]
    pub(super) phase: crate::TimeAcquisitionPhase,
    pub(super) staged_release_key: Option<String>,
    pub(super) canonical_json: String,
    pub(super) record_version: i64,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub(super) struct TimeCalendarVersionContent {
    pub(super) provenance: TimeProvenanceRecord,
    pub(super) tenant: RecordId,
    pub(super) owner: RecordId,
    pub(super) calendar_key: String,
    pub(super) calendar_version: i64,
    pub(super) name: String,
    pub(super) zone_id: String,
    pub(super) state: TimeCalendarState,
    pub(super) canonical_json: String,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub(super) struct TimeMissionEpochContent {
    pub(super) provenance: TimeProvenanceRecord,
    pub(super) tenant: RecordId,
    pub(super) owner: RecordId,
    pub(super) epoch_key: String,
    pub(super) name: String,
    pub(super) epoch_version: i64,
    pub(super) tai_seconds_since_1970: i64,
    pub(super) nanosecond: i64,
    pub(super) canonical_json: String,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub(super) struct TimeTemporalEventContent {
    pub(super) provenance: TimeProvenanceRecord,
    pub(super) tenant: RecordId,
    pub(super) owner: RecordId,
    pub(super) event_key: String,
    pub(super) name: String,
    pub(super) state: TimeTemporalEventState,
    pub(super) due_tai_seconds_since_1970: i64,
    pub(super) due_nanosecond: i64,
    pub(super) idempotency_key: String,
    pub(super) canonical_json: String,
    pub(super) record_version: i64,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub(super) struct TimeClockPolicyContent {
    pub(super) tenant: RecordId,
    pub(super) owner: RecordId,
    pub(super) maximum_error_nanoseconds: i64,
    pub(super) maximum_stratum: i64,
    pub(super) minimum_source_diversity: i64,
    pub(super) maximum_holdover_seconds: i64,
    pub(super) record_version: i64,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod vocabulary_baseline;
