use anyhow::{Context, Result};
use veoveo_platform_store::{
    PlatformIdentity, PlatformStore, TimeAcquisitionDraft, TimeAcquisitionRecord,
    TimeAcquisitionState as StoreAcquisitionState, TimeAuthorityReleaseDraft,
    TimeAuthorityReleaseRecord, TimeAuthorityReleaseState as StoreReleaseState, TimeCalendarState,
    TimeCalendarVersionDraft, TimeClockPolicyDraft, TimeDatasetKind, TimeMissionEpochDraft,
    TimeSourceDraft, TimeSourceRecord, TimeTemporalEventDraft, TimeTemporalEventRecord,
    TimeTemporalEventState as StoreEventState,
};

use crate::contract::{
    AuthorityRelease, AuthorityReleaseState, CalendarId, ClockQualityPolicy, MissionEpoch,
    OperationalCalendar, TemporalEvent, TemporalEventId, TemporalEventState, TimeAcquisition,
    TimeAcquisitionId, TimeAcquisitionStatus, TimeSource, TimeSourceId,
};

#[derive(Clone, Debug)]
pub struct TimeScope {
    pub identity: PlatformIdentity,
}

impl TimeScope {
    pub fn tenant_key(&self) -> String {
        self.identity.tenant_id.to_string()
    }
}

#[derive(Clone)]
pub struct TimeCatalog {
    store: PlatformStore,
}

impl TimeCatalog {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &PlatformStore {
        &self.store
    }

    pub async fn create_source(
        &self,
        scope: &TimeScope,
        mut source: TimeSource,
    ) -> Result<TimeSource> {
        source.record_version = 1;
        let canonical_json = serde_json::to_string(&source)?;
        let record = self
            .store
            .create_time_source(TimeSourceDraft {
                identity: scope.identity.clone(),
                source_key: source.source_id.to_string(),
                name: source.name.clone(),
                dataset_kind: source_kind(source.dataset_kind),
                source_url: source.url.clone(),
                expected_content_type: source.expected_content_type.clone(),
                enabled: source.enabled,
                canonical_json,
            })
            .await?;
        source_from_record(record)
    }

    pub async fn replace_source(
        &self,
        scope: &TimeScope,
        mut source: TimeSource,
        expected: u64,
    ) -> Result<TimeSource> {
        source.record_version = expected + 1;
        let canonical_json = serde_json::to_string(&source)?;
        let record = self
            .store
            .replace_time_source(
                TimeSourceDraft {
                    identity: scope.identity.clone(),
                    source_key: source.source_id.to_string(),
                    name: source.name.clone(),
                    dataset_kind: source_kind(source.dataset_kind),
                    source_url: source.url.clone(),
                    expected_content_type: source.expected_content_type.clone(),
                    enabled: source.enabled,
                    canonical_json,
                },
                expected.try_into()?,
            )
            .await?;
        source_from_record(record)
    }

    pub async fn source(&self, scope: &TimeScope, id: &TimeSourceId) -> Result<Option<TimeSource>> {
        self.store
            .time_source(scope.identity.tenant_id, id.as_str())
            .await?
            .map(source_from_record)
            .transpose()
    }

    pub async fn list_sources(&self, scope: &TimeScope) -> Result<Vec<TimeSource>> {
        self.store
            .list_time_sources(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(source_from_record)
            .collect()
    }

    pub async fn create_release(
        &self,
        scope: &TimeScope,
        release: AuthorityRelease,
    ) -> Result<AuthorityRelease> {
        let canonical_json = serde_json::to_string(&release)?;
        let record = self
            .store
            .create_time_authority_release(TimeAuthorityReleaseDraft {
                identity: scope.identity.clone(),
                release_key: release.release_id.to_string(),
                source_key: release.source_id.to_string(),
                dataset_kind: source_kind(release.dataset_kind),
                state: release_state(release.state),
                version_label: release.version_label.clone(),
                source_url: release.source_url.clone(),
                source_digest_sha256: release.source_digest_sha256.clone(),
                artifact_path: release.artifact_path.clone(),
                retrieved_at: release.retrieved_at,
                validated_at: release.validated_at,
                canonical_json,
            })
            .await?;
        release_from_record(record)
    }

    pub async fn release(
        &self,
        scope: &TimeScope,
        id: &crate::contract::AuthorityReleaseId,
    ) -> Result<Option<AuthorityRelease>> {
        self.store
            .time_authority_release(scope.identity.tenant_id, id.as_str())
            .await?
            .map(release_from_record)
            .transpose()
    }

    pub async fn list_releases(&self, scope: &TimeScope) -> Result<Vec<AuthorityRelease>> {
        self.store
            .list_time_authority_releases(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(release_from_record)
            .collect()
    }

    pub async fn activate_release(
        &self,
        scope: &TimeScope,
        id: &crate::contract::AuthorityReleaseId,
        expected_release: u64,
        expected_pointer: u64,
    ) -> Result<AuthorityRelease> {
        let mut release = self
            .release(scope, id)
            .await?
            .context("unknown authority release")?;
        if release.state != AuthorityReleaseState::Staged {
            anyhow::bail!("only a staged authority release can be activated");
        }
        release.state = AuthorityReleaseState::Active;
        release.record_version = expected_release + 1;
        let canonical_json = serde_json::to_string(&release)?;
        let record = self
            .store
            .activate_time_authority_release(
                &scope.identity,
                id.as_str(),
                expected_release.try_into()?,
                expected_pointer.try_into()?,
                canonical_json,
            )
            .await?;
        release_from_record(record)
    }

    pub async fn active_releases(&self, scope: &TimeScope) -> Result<Vec<AuthorityRelease>> {
        let pointers = self
            .store
            .list_active_time_authorities(scope.identity.tenant_id)
            .await?;
        let mut releases = Vec::new();
        for pointer in pointers {
            if let Some(record) = self
                .store
                .time_authority_release(scope.identity.tenant_id, &pointer.release_key)
                .await?
            {
                releases.push(release_from_record(record)?);
            }
        }
        Ok(releases)
    }

    pub async fn create_acquisition(
        &self,
        scope: &TimeScope,
        acquisition: TimeAcquisition,
        idempotency_key: String,
    ) -> Result<TimeAcquisition> {
        let canonical_json = serde_json::to_string(&acquisition)?;
        let record = self
            .store
            .create_time_acquisition(TimeAcquisitionDraft {
                identity: scope.identity.clone(),
                acquisition_key: acquisition.acquisition_id.to_string(),
                source_key: acquisition.source_id.to_string(),
                expected_source_digest_sha256: acquisition.expected_source_digest_sha256.clone(),
                idempotency_key,
                status: acquisition_state(acquisition.status),
                phase: acquisition.phase.clone(),
                staged_release_key: acquisition
                    .staged_release_id
                    .as_ref()
                    .map(ToString::to_string),
                canonical_json,
            })
            .await?;
        acquisition_from_record(record)
    }

    pub async fn acquisition(
        &self,
        scope: &TimeScope,
        id: &TimeAcquisitionId,
    ) -> Result<Option<TimeAcquisition>> {
        self.store
            .time_acquisition(scope.identity.tenant_id, id.as_str())
            .await?
            .map(acquisition_from_record)
            .transpose()
    }

    pub async fn acquisition_for_idempotency(
        &self,
        scope: &TimeScope,
        idempotency_key: &str,
    ) -> Result<Option<TimeAcquisition>> {
        self.store
            .time_acquisition_for_idempotency(
                scope.identity.tenant_id,
                scope.identity.principal_id.record_id(),
                idempotency_key,
            )
            .await?
            .map(acquisition_from_record)
            .transpose()
    }

    pub async fn list_acquisitions(&self, scope: &TimeScope) -> Result<Vec<TimeAcquisition>> {
        self.store
            .list_time_acquisitions(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(acquisition_from_record)
            .collect()
    }

    pub async fn update_acquisition(
        &self,
        scope: &TimeScope,
        mut acquisition: TimeAcquisition,
    ) -> Result<TimeAcquisition> {
        let expected = acquisition.record_version;
        acquisition.record_version += 1;
        acquisition.updated_at = chrono::Utc::now();
        let canonical_json = serde_json::to_string(&acquisition)?;
        let record = self
            .store
            .update_time_acquisition(
                scope.identity.tenant_id,
                acquisition.acquisition_id.as_str(),
                expected.try_into()?,
                acquisition_state(acquisition.status),
                acquisition.phase.clone(),
                acquisition
                    .staged_release_id
                    .as_ref()
                    .map(ToString::to_string),
                canonical_json,
            )
            .await?;
        acquisition_from_record(record)
    }

    pub async fn create_calendar(
        &self,
        scope: &TimeScope,
        calendar: OperationalCalendar,
    ) -> Result<OperationalCalendar> {
        let canonical_json = serde_json::to_string(&calendar)?;
        let record = self
            .store
            .create_time_calendar_version(TimeCalendarVersionDraft {
                identity: scope.identity.clone(),
                calendar_key: calendar.calendar_id.to_string(),
                calendar_version: calendar.version.try_into()?,
                name: calendar.name.clone(),
                zone_id: calendar.zone_id.clone(),
                state: TimeCalendarState::Active,
                canonical_json,
            })
            .await?;
        serde_json::from_str(&record.canonical_json).context("decoding stored operational calendar")
    }

    pub async fn calendar(
        &self,
        scope: &TimeScope,
        id: &CalendarId,
        version: u64,
    ) -> Result<Option<OperationalCalendar>> {
        self.store
            .time_calendar_version(scope.identity.tenant_id, id.as_str(), version.try_into()?)
            .await?
            .map(|record| {
                serde_json::from_str(&record.canonical_json)
                    .context("decoding stored operational calendar")
            })
            .transpose()
    }

    pub async fn list_calendars(&self, scope: &TimeScope) -> Result<Vec<OperationalCalendar>> {
        self.store
            .list_time_calendar_versions(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(|record| {
                serde_json::from_str(&record.canonical_json)
                    .context("decoding stored operational calendar")
            })
            .collect()
    }

    pub async fn create_epoch(
        &self,
        scope: &TimeScope,
        epoch: MissionEpoch,
    ) -> Result<MissionEpoch> {
        let canonical_json = serde_json::to_string(&epoch)?;
        let record = self
            .store
            .create_time_mission_epoch(TimeMissionEpochDraft {
                identity: scope.identity.clone(),
                epoch_key: epoch.epoch_id.to_string(),
                name: epoch.name.clone(),
                epoch_version: epoch.version.try_into()?,
                tai_seconds_since_1970: epoch.instant.tai_seconds_since_1970,
                nanosecond: i64::from(epoch.instant.nanosecond),
                canonical_json,
            })
            .await?;
        serde_json::from_str(&record.canonical_json).context("decoding stored mission epoch")
    }

    pub async fn epoch(
        &self,
        scope: &TimeScope,
        id: &crate::contract::MissionEpochId,
    ) -> Result<Option<MissionEpoch>> {
        let records = self
            .store
            .list_time_mission_epochs(scope.identity.tenant_id)
            .await?;
        records
            .into_iter()
            .find(|record| record.epoch_key == id.as_str())
            .map(|record| {
                serde_json::from_str(&record.canonical_json)
                    .context("decoding stored mission epoch")
            })
            .transpose()
    }

    pub async fn list_epochs(&self, scope: &TimeScope) -> Result<Vec<MissionEpoch>> {
        self.store
            .list_time_mission_epochs(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(|record| {
                serde_json::from_str(&record.canonical_json)
                    .context("decoding stored mission epoch")
            })
            .collect()
    }

    pub async fn create_event(
        &self,
        scope: &TimeScope,
        event: TemporalEvent,
        idempotency_key: String,
    ) -> Result<TemporalEvent> {
        let canonical_json = serde_json::to_string(&event)?;
        let record = self
            .store
            .create_time_temporal_event(TimeTemporalEventDraft {
                identity: scope.identity.clone(),
                event_key: event.event_id.to_string(),
                name: event.name.clone(),
                state: event_state(event.state),
                due_tai_seconds_since_1970: event.due.tai_seconds_since_1970,
                due_nanosecond: i64::from(event.due.nanosecond),
                idempotency_key,
                canonical_json,
            })
            .await?;
        event_from_record(record)
    }

    pub async fn event(
        &self,
        scope: &TimeScope,
        id: &TemporalEventId,
    ) -> Result<Option<TemporalEvent>> {
        let record = self
            .store
            .time_temporal_event(scope.identity.tenant_id, id.as_str())
            .await?;
        let Some(record) = record else {
            return Ok(None);
        };
        if record.owner != scope.identity.principal_id.record_id() {
            return Ok(None);
        }
        Ok(Some(event_from_record(record)?))
    }

    pub async fn list_events(&self, scope: &TimeScope) -> Result<Vec<TemporalEvent>> {
        self.store
            .list_time_temporal_events(scope.identity.tenant_id)
            .await?
            .into_iter()
            .filter(|record| record.owner == scope.identity.principal_id.record_id())
            .map(event_from_record)
            .collect()
    }

    pub async fn cancel_event(
        &self,
        scope: &TimeScope,
        id: &TemporalEventId,
        expected: u64,
    ) -> Result<TemporalEvent> {
        let mut event = self
            .event(scope, id)
            .await?
            .context("unknown temporal event")?;
        event.state = TemporalEventState::Cancelled;
        event.record_version = expected + 1;
        let record = self
            .store
            .transition_time_temporal_event(
                scope.identity.tenant_id,
                id.as_str(),
                expected.try_into()?,
                StoreEventState::Cancelled,
                serde_json::to_string(&event)?,
            )
            .await?;
        event_from_record(record)
    }

    pub async fn mark_event_due(
        &self,
        scope: &TimeScope,
        id: &TemporalEventId,
        expected: u64,
    ) -> Result<TemporalEvent> {
        let mut event = self
            .event(scope, id)
            .await?
            .context("unknown temporal event")?;
        if event.state != TemporalEventState::Scheduled {
            return Ok(event);
        }
        event.state = TemporalEventState::Due;
        event.record_version = expected + 1;
        let record = self
            .store
            .transition_time_temporal_event(
                scope.identity.tenant_id,
                id.as_str(),
                expected.try_into()?,
                StoreEventState::Due,
                serde_json::to_string(&event)?,
            )
            .await?;
        event_from_record(record)
    }

    pub async fn clock_policy(
        &self,
        scope: &TimeScope,
    ) -> Result<Option<(ClockQualityPolicy, u64)>> {
        Ok(self
            .store
            .time_clock_policy(scope.identity.tenant_id)
            .await?
            .map(|record| {
                (
                    ClockQualityPolicy {
                        maximum_error_nanoseconds: record.maximum_error_nanoseconds as u64,
                        maximum_stratum: record.maximum_stratum as u8,
                        minimum_source_diversity: record.minimum_source_diversity as u32,
                        maximum_holdover_seconds: record.maximum_holdover_seconds as u64,
                    },
                    record.record_version as u64,
                )
            }))
    }

    pub async fn replace_clock_policy(
        &self,
        scope: &TimeScope,
        policy: ClockQualityPolicy,
        expected: u64,
    ) -> Result<(ClockQualityPolicy, u64)> {
        let record = self
            .store
            .replace_time_clock_policy(
                TimeClockPolicyDraft {
                    identity: scope.identity.clone(),
                    maximum_error_nanoseconds: policy.maximum_error_nanoseconds.try_into()?,
                    maximum_stratum: i64::from(policy.maximum_stratum),
                    minimum_source_diversity: i64::from(policy.minimum_source_diversity),
                    maximum_holdover_seconds: policy.maximum_holdover_seconds.try_into()?,
                },
                expected.try_into()?,
            )
            .await?;
        Ok((policy, record.record_version.try_into()?))
    }
}

fn source_from_record(record: TimeSourceRecord) -> Result<TimeSource> {
    let mut value: TimeSource = serde_json::from_str(&record.canonical_json)?;
    value.record_version = record.record_version.try_into()?;
    Ok(value)
}
fn release_from_record(record: TimeAuthorityReleaseRecord) -> Result<AuthorityRelease> {
    let mut value: AuthorityRelease = serde_json::from_str(&record.canonical_json)?;
    value.state = release_state_from_store(record.state);
    value.record_version = record.record_version.try_into()?;
    Ok(value)
}
fn acquisition_from_record(record: TimeAcquisitionRecord) -> Result<TimeAcquisition> {
    let mut value: TimeAcquisition = serde_json::from_str(&record.canonical_json)?;
    value.status = acquisition_state_from_store(record.status);
    value.phase = record.phase;
    value.staged_release_id = record
        .staged_release_key
        .map(TimeAcquisitionReleaseId::parse)
        .transpose()?;
    value.record_version = record.record_version.try_into()?;
    value.updated_at = record.updated_at;
    Ok(value)
}
fn event_from_record(record: TimeTemporalEventRecord) -> Result<TemporalEvent> {
    let mut value: TemporalEvent = serde_json::from_str(&record.canonical_json)?;
    value.state = event_state_from_store(record.state);
    value.record_version = record.record_version.try_into()?;
    Ok(value)
}

struct TimeAcquisitionReleaseId;
impl TimeAcquisitionReleaseId {
    fn parse(value: String) -> Result<crate::contract::AuthorityReleaseId> {
        crate::contract::AuthorityReleaseId::new(value).map_err(anyhow::Error::msg)
    }
}

fn source_kind(value: crate::contract::AuthorityDatasetKind) -> TimeDatasetKind {
    match value {
        crate::contract::AuthorityDatasetKind::Tzdb => TimeDatasetKind::Tzdb,
        crate::contract::AuthorityDatasetKind::LeapSeconds => TimeDatasetKind::LeapSeconds,
    }
}
fn release_state(value: AuthorityReleaseState) -> StoreReleaseState {
    match value {
        AuthorityReleaseState::Staged => StoreReleaseState::Staged,
        AuthorityReleaseState::Active => StoreReleaseState::Active,
        AuthorityReleaseState::Retired => StoreReleaseState::Retired,
        AuthorityReleaseState::Quarantined => StoreReleaseState::Quarantined,
    }
}
fn release_state_from_store(value: StoreReleaseState) -> AuthorityReleaseState {
    match value {
        StoreReleaseState::Staged => AuthorityReleaseState::Staged,
        StoreReleaseState::Active => AuthorityReleaseState::Active,
        StoreReleaseState::Retired => AuthorityReleaseState::Retired,
        StoreReleaseState::Quarantined => AuthorityReleaseState::Quarantined,
    }
}
fn acquisition_state(value: TimeAcquisitionStatus) -> StoreAcquisitionState {
    match value {
        TimeAcquisitionStatus::Queued => StoreAcquisitionState::Queued,
        TimeAcquisitionStatus::Running => StoreAcquisitionState::Running,
        TimeAcquisitionStatus::Succeeded => StoreAcquisitionState::Succeeded,
        TimeAcquisitionStatus::Failed => StoreAcquisitionState::Failed,
        TimeAcquisitionStatus::CancelRequested => StoreAcquisitionState::CancelRequested,
        TimeAcquisitionStatus::Cancelled => StoreAcquisitionState::Cancelled,
    }
}
fn acquisition_state_from_store(value: StoreAcquisitionState) -> TimeAcquisitionStatus {
    match value {
        StoreAcquisitionState::Queued => TimeAcquisitionStatus::Queued,
        StoreAcquisitionState::Running => TimeAcquisitionStatus::Running,
        StoreAcquisitionState::Succeeded => TimeAcquisitionStatus::Succeeded,
        StoreAcquisitionState::Failed => TimeAcquisitionStatus::Failed,
        StoreAcquisitionState::CancelRequested => TimeAcquisitionStatus::CancelRequested,
        StoreAcquisitionState::Cancelled => TimeAcquisitionStatus::Cancelled,
    }
}
fn event_state(value: TemporalEventState) -> StoreEventState {
    match value {
        TemporalEventState::Scheduled => StoreEventState::Scheduled,
        TemporalEventState::Due => StoreEventState::Due,
        TemporalEventState::Cancelled => StoreEventState::Cancelled,
    }
}
fn event_state_from_store(value: StoreEventState) -> TemporalEventState {
    match value {
        StoreEventState::Scheduled => TemporalEventState::Scheduled,
        StoreEventState::Due => TemporalEventState::Due,
        StoreEventState::Cancelled => TemporalEventState::Cancelled,
    }
}
