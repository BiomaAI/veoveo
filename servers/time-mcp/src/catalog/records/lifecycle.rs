//! Decoder for retained lifecycle bodies. Stored columns supply the current version.
//! The historical body's unsigned version is admitted but never becomes current state.
use crate::contract::*;
use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct TimeSourceBody {
    source_id: TimeSourceId,
    name: String,
    dataset_kind: AuthorityDatasetKind,
    url: String,
    expected_content_type: String,
    enabled: bool,
    #[serde(rename = "record_version")]
    _historical_version: u64,
}

impl TimeSourceBody {
    pub(super) fn into_current(self, record_version: TimeVersion) -> TimeSource {
        TimeSource {
            source_id: self.source_id,
            name: self.name,
            dataset_kind: self.dataset_kind,
            url: self.url,
            expected_content_type: self.expected_content_type,
            enabled: self.enabled,
            record_version,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct AuthorityReleaseBody {
    release_id: AuthorityReleaseId,
    source_id: TimeSourceId,
    dataset_kind: AuthorityDatasetKind,
    version_label: String,
    source_url: String,
    source_digest_sha256: AuthoritySourceDigest,
    artifact_path: String,
    state: AuthorityReleaseState,
    retrieved_at: DateTime<Utc>,
    validated_at: DateTime<Utc>,
    #[serde(rename = "record_version")]
    _historical_version: u64,
}

impl AuthorityReleaseBody {
    pub(super) fn into_current(self, record_version: TimeVersion) -> AuthorityRelease {
        AuthorityRelease {
            release_id: self.release_id,
            source_id: self.source_id,
            dataset_kind: self.dataset_kind,
            version_label: self.version_label,
            source_url: self.source_url,
            source_digest_sha256: self.source_digest_sha256,
            artifact_path: self.artifact_path,
            state: self.state,
            retrieved_at: self.retrieved_at,
            validated_at: self.validated_at,
            record_version,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct TimeAcquisitionBody {
    acquisition_id: TimeAcquisitionId,
    source_id: TimeSourceId,
    expected_source_digest_sha256: Option<AuthoritySourceDigest>,
    status: TimeAcquisitionStatus,
    phase: String,
    staged_release_id: Option<AuthorityReleaseId>,
    message: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(rename = "record_version")]
    _historical_version: u64,
}

impl TimeAcquisitionBody {
    pub(super) fn into_current(self, record_version: TimeVersion) -> TimeAcquisition {
        TimeAcquisition {
            acquisition_id: self.acquisition_id,
            source_id: self.source_id,
            expected_source_digest_sha256: self.expected_source_digest_sha256,
            status: self.status,
            phase: self.phase,
            staged_release_id: self.staged_release_id,
            message: self.message,
            created_at: self.created_at,
            updated_at: self.updated_at,
            record_version,
        }
    }
}

#[derive(Deserialize)]
pub(super) struct TemporalEventBody {
    event_id: TemporalEventId,
    name: String,
    due: TimeInstant,
    state: TemporalEventState,
    #[serde(rename = "record_version")]
    _historical_version: u64,
}

impl TemporalEventBody {
    pub(super) fn into_current(self, record_version: TimeVersion) -> TemporalEvent {
        TemporalEvent {
            event_id: self.event_id,
            name: self.name,
            due: self.due,
            state: self.state,
            record_version,
        }
    }
}
