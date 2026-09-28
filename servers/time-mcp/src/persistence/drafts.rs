use super::*;

#[derive(Clone, Debug)]
pub(crate) struct TimeSourceDraft {
    pub(crate) identity: PlatformIdentity,
    pub(crate) source_key: TimeSourceId,
    pub(crate) name: String,
    pub(crate) dataset_kind: TimeDatasetKind,
    pub(crate) source_url: String,
    pub(crate) expected_content_type: String,
    pub(crate) enabled: bool,
    pub(crate) canonical_json: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeAuthorityReleaseDraft {
    pub(crate) identity: PlatformIdentity,
    pub(crate) release_key: AuthorityReleaseId,
    pub(crate) source_key: TimeSourceId,
    pub(crate) dataset_kind: TimeDatasetKind,
    pub(crate) state: TimeAuthorityReleaseState,
    pub(crate) version_label: String,
    pub(crate) source_url: String,
    pub(crate) source_digest_sha256: String,
    pub(crate) artifact_path: String,
    pub(crate) retrieved_at: DateTime<Utc>,
    pub(crate) validated_at: DateTime<Utc>,
    pub(crate) canonical_json: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeAcquisitionDraft {
    pub(crate) identity: PlatformIdentity,
    pub(crate) acquisition_key: TimeAcquisitionId,
    pub(crate) source_key: TimeSourceId,
    pub(crate) expected_source_digest_sha256: Option<String>,
    pub(crate) idempotency_key: String,
    pub(crate) status: TimeAcquisitionState,
    pub(crate) phase: String,
    pub(crate) staged_release_key: Option<AuthorityReleaseId>,
    pub(crate) canonical_json: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeAcquisitionUpdate {
    pub(crate) tenant_id: TenantId,
    pub(crate) acquisition_key: TimeAcquisitionId,
    pub(crate) expected_record_version: i64,
    pub(crate) status: TimeAcquisitionState,
    pub(crate) phase: String,
    pub(crate) staged_release_key: Option<AuthorityReleaseId>,
    pub(crate) canonical_json: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeCalendarVersionDraft {
    pub(crate) identity: PlatformIdentity,
    pub(crate) calendar_key: CalendarId,
    pub(crate) calendar_version: TimeVersion,
    pub(crate) name: String,
    pub(crate) zone_id: String,
    pub(crate) state: TimeCalendarState,
    pub(crate) canonical_json: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeMissionEpochDraft {
    pub(crate) identity: PlatformIdentity,
    pub(crate) epoch_key: MissionEpochId,
    pub(crate) name: String,
    pub(crate) epoch_version: TimeVersion,
    pub(crate) tai_seconds_since_1970: i64,
    pub(crate) nanosecond: i64,
    pub(crate) canonical_json: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeTemporalEventDraft {
    pub(crate) identity: PlatformIdentity,
    pub(crate) event_key: TemporalEventId,
    pub(crate) name: String,
    pub(crate) state: TimeTemporalEventState,
    pub(crate) due_tai_seconds_since_1970: i64,
    pub(crate) due_nanosecond: i64,
    pub(crate) idempotency_key: String,
    pub(crate) canonical_json: String,
}

#[derive(Clone, Debug)]
pub(crate) struct TimeClockPolicyDraft {
    pub(crate) identity: PlatformIdentity,
    pub(crate) maximum_error_nanoseconds: i64,
    pub(crate) maximum_stratum: i64,
    pub(crate) minimum_source_diversity: i64,
    pub(crate) maximum_holdover_seconds: i64,
}
