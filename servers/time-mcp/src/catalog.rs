mod activation;
pub(crate) use activation::ActivationDraft;
mod clock;
pub mod knowledge;
mod pages;
mod records;

use records::{
    acquisition_from_record, calendar_from_record, epoch_from_record, event_from_record,
    release_from_record, source_from_record,
};
#[cfg(test)]
mod tests;

/// Store-backed completion domains. SQL identifiers come only from this enum.
/// ```compile_fail
/// use veoveo_time_mcp::{catalog::TimeCompletion, MissionEpochId};
/// TimeCompletion::CalendarVersion {
///     calendar_key: Some(MissionEpochId::parse("epoch-example").unwrap()),
/// };
/// ```
/// ```compile_fail
/// use veoveo_time_mcp::catalog::TimeCompletion;
/// TimeCompletion::CalendarVersion { calendar_key: Some("calendar-example".to_owned()) };
/// ```
#[derive(Clone, Debug)]
pub enum TimeCompletion {
    CalendarId,
    CalendarVersion {
        calendar_key: Option<CalendarId>,
    },
    EpochId,
    EpochVersion {
        epoch_key: Option<crate::MissionEpochId>,
    },
    AuthorityReleaseId,
    EventId,
}

use crate::persistence::{
    TimeAcquisitionDraft, TimeAcquisitionState as StoreAcquisitionState, TimeAcquisitionUpdate,
    TimeAuthorityReleaseDraft, TimeAuthorityReleaseState as StoreReleaseState, TimeCalendarState,
    TimeCalendarVersionDraft, TimeDatasetKind, TimeMissionEpochDraft, TimePersistence,
    TimeSourceDraft, TimeTemporalEventDraft, TimeTemporalEventState as StoreEventState,
};
use anyhow::{Context, Result, bail};
use veoveo_platform_store::{PlatformIdentity, PlatformStore};

use crate::contract::{
    AuthorityRelease, AuthorityReleaseState, CalendarId, MissionEpoch, NewTimeSource,
    OperationalCalendar, TemporalEvent, TemporalEventId, TemporalEventState, TimeAcquisition,
    TimeAcquisitionId, TimeAcquisitionStatus, TimeAuthorityReference, TimeAuthorityReleaseUri,
    TimeAuthoritySource, TimeSource, TimeSourceId,
};

#[derive(Clone, Debug)]
pub struct TimeAccessContext {
    pub identity: PlatformIdentity,
    pub work_context: veoveo_types::WorkContextId,
}

impl TimeAccessContext {
    pub fn tenant_key(&self) -> String {
        self.identity.tenant_id.to_string()
    }
}

#[derive(Clone)]
pub struct TimeCatalog {
    persistence: TimePersistence,
}

impl TimeCatalog {
    pub fn new(store: PlatformStore) -> Self {
        Self {
            persistence: TimePersistence::new(store),
        }
    }

    pub async fn complete_values(
        &self,
        scope: &TimeAccessContext,
        domain: TimeCompletion,
        needle: &str,
        limit: u32,
    ) -> Result<Vec<String>> {
        Ok(self
            .persistence
            .complete_time_values(&scope.identity, domain, needle, limit)
            .await?)
    }

    pub async fn create_source(
        &self,
        scope: &TimeAccessContext,
        source: NewTimeSource,
    ) -> Result<TimeSource> {
        let source = crate::NewTimeSourceValue::from(source);
        let source = crate::TimeSourceValue {
            source_id: source.source_id,
            name: source.name,
            dataset_kind: source.dataset_kind,
            url: source.url,
            expected_content_type: source.expected_content_type,
            enabled: source.enabled,
            record_version: crate::TimeVersion::FIRST,
        }
        .build()?;
        let canonical_json = serde_json::to_string(&source)?;
        let record = self
            .persistence
            .create_time_source(TimeSourceDraft {
                identity: scope.identity.clone(),
                source_key: source.source_id.clone(),
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
        scope: &TimeAccessContext,
        source: TimeSource,
        expected: crate::TimeVersion,
    ) -> Result<TimeSource> {
        let mut source = crate::TimeSourceValue::from(source);
        source.record_version = expected.checked_next()?;
        let source = source.build()?;
        let canonical_json = serde_json::to_string(&source)?;
        let record = self
            .persistence
            .replace_time_source(
                TimeSourceDraft {
                    identity: scope.identity.clone(),
                    source_key: source.source_id.clone(),
                    name: source.name.clone(),
                    dataset_kind: source_kind(source.dataset_kind),
                    source_url: source.url.clone(),
                    expected_content_type: source.expected_content_type.clone(),
                    enabled: source.enabled,
                    canonical_json,
                },
                expected,
            )
            .await?;
        source_from_record(record)
    }

    pub async fn source(
        &self,
        scope: &TimeAccessContext,
        id: &TimeSourceId,
    ) -> Result<Option<TimeSource>> {
        self.persistence
            .time_source(scope.identity.tenant_id, id)
            .await?
            .map(source_from_record)
            .transpose()
    }

    pub async fn list_sources(&self, scope: &TimeAccessContext) -> Result<Vec<TimeSource>> {
        self.persistence
            .list_time_sources(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(source_from_record)
            .collect()
    }

    pub async fn create_release(
        &self,
        scope: &TimeAccessContext,
        release: AuthorityRelease,
    ) -> Result<AuthorityRelease> {
        let canonical_json = serde_json::to_string(&release)?;
        let record = self
            .persistence
            .create_time_authority_release(TimeAuthorityReleaseDraft {
                work_context: scope.work_context.clone(),
                identity: scope.identity.clone(),
                release_key: release.release_id.clone(),
                source_key: release.source_id.clone(),
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
        scope: &TimeAccessContext,
        id: &crate::contract::AuthorityReleaseId,
    ) -> Result<Option<AuthorityRelease>> {
        self.persistence
            .time_authority_release(scope.identity.tenant_id, id)
            .await?
            .map(release_from_record)
            .transpose()
    }

    pub async fn list_releases(&self, scope: &TimeAccessContext) -> Result<Vec<AuthorityRelease>> {
        self.persistence
            .list_time_authority_releases(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(release_from_record)
            .collect()
    }

    pub async fn active_releases(
        &self,
        scope: &TimeAccessContext,
    ) -> Result<Vec<AuthorityRelease>> {
        self.persistence
            .list_active_time_authorities(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(|active| release_from_record(active.release))
            .collect()
    }

    pub async fn authority_reference(
        &self,
        scope: &TimeAccessContext,
        release: &AuthorityRelease,
    ) -> Result<TimeAuthorityReference> {
        let acquisition = self
            .acquisition_for_release(scope, &release.release_id)
            .await?
            .with_context(|| {
                format!(
                    "authority release `{}` has no producing acquisition",
                    release.release_id
                )
            })?;
        if acquisition.source_id != release.source_id
            || acquisition.staged_release_id.as_ref() != Some(&release.release_id)
        {
            bail!(
                "authority release `{}` provenance does not match its producing acquisition",
                release.release_id
            );
        }
        Ok(TimeAuthorityReference::new(
            TimeAuthorityReleaseUri::new(&release.release_id),
            release.dataset_kind,
            TimeAuthoritySource::Acquisition {
                source_id: release.source_id.clone(),
                acquisition_id: acquisition.acquisition_id,
            },
            release.source_digest_sha256.canonical().clone(),
            release.version_label.clone(),
        )?)
    }

    pub async fn create_acquisition(
        &self,
        scope: &TimeAccessContext,
        acquisition: TimeAcquisition,
        idempotency_key: String,
    ) -> Result<TimeAcquisition> {
        veoveo_types::Check::check(&acquisition)?;
        let canonical_json = serde_json::to_string(&acquisition)?;
        let record = self
            .persistence
            .create_time_acquisition(TimeAcquisitionDraft {
                identity: scope.identity.clone(),
                acquisition_key: acquisition.acquisition_id.clone(),
                source_key: acquisition.source_id.clone(),
                expected_source_digest_sha256: acquisition.expected_source_digest_sha256.clone(),
                idempotency_key,
                status: acquisition_state(acquisition.status),
                phase: acquisition.phase,
                staged_release_key: acquisition.staged_release_id.clone(),
                canonical_json,
            })
            .await?;
        acquisition_from_record(record)
    }

    pub async fn acquisition(
        &self,
        scope: &TimeAccessContext,
        id: &TimeAcquisitionId,
    ) -> Result<Option<TimeAcquisition>> {
        self.persistence
            .time_acquisition(scope.identity.tenant_id, id)
            .await?
            .map(acquisition_from_record)
            .transpose()
    }

    pub async fn acquisition_for_release(
        &self,
        scope: &TimeAccessContext,
        release_id: &crate::contract::AuthorityReleaseId,
    ) -> Result<Option<TimeAcquisition>> {
        self.persistence
            .time_acquisition_for_release(scope.identity.tenant_id, release_id)
            .await?
            .map(acquisition_from_record)
            .transpose()
    }

    pub async fn acquisition_for_idempotency(
        &self,
        scope: &TimeAccessContext,
        idempotency_key: &str,
    ) -> Result<Option<TimeAcquisition>> {
        self.persistence
            .time_acquisition_for_idempotency(
                scope.identity.tenant_id,
                scope.identity.principal_id,
                idempotency_key,
            )
            .await?
            .map(acquisition_from_record)
            .transpose()
    }

    pub async fn list_acquisitions(
        &self,
        scope: &TimeAccessContext,
    ) -> Result<Vec<TimeAcquisition>> {
        self.persistence
            .list_time_acquisitions(scope.identity.tenant_id)
            .await?
            .into_iter()
            .map(acquisition_from_record)
            .collect()
    }

    pub async fn update_acquisition(
        &self,
        scope: &TimeAccessContext,
        mut acquisition: TimeAcquisition,
    ) -> Result<TimeAcquisition> {
        veoveo_types::Check::check(&acquisition)?;
        let expected = acquisition.record_version;
        let next = expected.checked_next()?;
        let current = self
            .acquisition(scope, &acquisition.acquisition_id)
            .await?
            .context("unknown Time acquisition")?;
        anyhow::ensure!(
            acquisition.source_id == current.source_id
                && acquisition.expected_source_digest_sha256
                    == current.expected_source_digest_sha256
                && acquisition.created_at == current.created_at,
            "an acquisition update cannot change its source, expected digest or creation time"
        );
        acquisition.record_version = next;
        acquisition.updated_at = chrono::Utc::now();
        let canonical_json = serde_json::to_string(&acquisition)?;
        let record = self
            .persistence
            .update_time_acquisition(TimeAcquisitionUpdate {
                tenant_id: scope.identity.tenant_id,
                acquisition_key: acquisition.acquisition_id.clone(),
                expected_record_version: expected,
                status: acquisition_state(acquisition.status),
                phase: acquisition.phase,
                staged_release_key: acquisition.staged_release_id.clone(),
                canonical_json,
            })
            .await?;
        acquisition_from_record(record)
    }

    pub async fn create_calendar(
        &self,
        scope: &TimeAccessContext,
        calendar: OperationalCalendar,
    ) -> Result<OperationalCalendar> {
        let canonical_json = serde_json::to_string(&calendar)?;
        let record = self
            .persistence
            .create_time_calendar_version(TimeCalendarVersionDraft {
                work_context: scope.work_context.clone(),
                identity: scope.identity.clone(),
                calendar_key: calendar.calendar_id.clone(),
                calendar_version: calendar.version,
                name: calendar.name.clone(),
                zone_id: calendar.zone_id.clone(),
                state: TimeCalendarState::Active,
                canonical_json,
            })
            .await?;
        calendar_from_record(record)
    }

    pub async fn calendar(
        &self,
        scope: &TimeAccessContext,
        id: &CalendarId,
        version: crate::contract::TimeVersion,
    ) -> Result<Option<OperationalCalendar>> {
        self.persistence
            .time_calendar_version(scope.identity.tenant_id, id, version)
            .await?
            .map(calendar_from_record)
            .transpose()
    }

    pub async fn create_epoch(
        &self,
        scope: &TimeAccessContext,
        epoch: MissionEpoch,
    ) -> Result<MissionEpoch> {
        let canonical_json = serde_json::to_string(&epoch)?;
        let record = self
            .persistence
            .create_time_mission_epoch(TimeMissionEpochDraft {
                work_context: scope.work_context.clone(),
                identity: scope.identity.clone(),
                epoch_key: epoch.epoch_id.clone(),
                name: epoch.name.clone(),
                epoch_version: epoch.version,
                tai_seconds_since_1970: epoch.instant.tai_seconds_since_1970,
                nanosecond: epoch.instant.nanosecond,
                canonical_json,
            })
            .await?;
        epoch_from_record(record)
    }

    pub async fn epoch(
        &self,
        scope: &TimeAccessContext,
        id: &crate::contract::MissionEpochId,
    ) -> Result<Option<MissionEpoch>> {
        self.persistence
            .latest_time_mission_epoch(scope.identity.tenant_id, id)
            .await?
            .map(epoch_from_record)
            .transpose()
    }

    pub async fn create_event(
        &self,
        scope: &TimeAccessContext,
        event: TemporalEvent,
        idempotency_key: String,
    ) -> Result<TemporalEvent> {
        let canonical_json = serde_json::to_string(&event)?;
        let record = self
            .persistence
            .create_time_temporal_event(TimeTemporalEventDraft {
                work_context: scope.work_context.clone(),
                identity: scope.identity.clone(),
                event_key: event.event_id.clone(),
                name: event.name.clone(),
                state: event_state(event.state),
                due_tai_seconds_since_1970: event.due.tai_seconds_since_1970,
                due_nanosecond: event.due.nanosecond,
                idempotency_key,
                canonical_json,
            })
            .await?;
        event_from_record(record)
    }

    pub async fn event(
        &self,
        scope: &TimeAccessContext,
        id: &TemporalEventId,
    ) -> Result<Option<TemporalEvent>> {
        self.persistence
            .time_temporal_event(&scope.identity, id)
            .await?
            .map(event_from_record)
            .transpose()
    }

    pub async fn cancel_event(
        &self,
        scope: &TimeAccessContext,
        id: &TemporalEventId,
        expected: crate::TimeVersion,
    ) -> Result<TemporalEvent> {
        let mut event = self
            .event(scope, id)
            .await?
            .context("unknown temporal event")?;
        event.state = TemporalEventState::Cancelled;
        event.record_version = expected.checked_next()?;
        let record = self
            .persistence
            .transition_time_temporal_event(
                &scope.identity,
                id,
                expected,
                StoreEventState::Cancelled,
                serde_json::to_string(&event)?,
            )
            .await?;
        event_from_record(record)
    }

    pub async fn mark_event_due(
        &self,
        scope: &TimeAccessContext,
        id: &TemporalEventId,
        expected: crate::TimeVersion,
    ) -> Result<TemporalEvent> {
        let mut event = self
            .event(scope, id)
            .await?
            .context("unknown temporal event")?;
        if event.state != TemporalEventState::Scheduled {
            return Ok(event);
        }
        event.state = TemporalEventState::Due;
        event.record_version = expected.checked_next()?;
        let record = self
            .persistence
            .transition_time_temporal_event(
                &scope.identity,
                id,
                expected,
                StoreEventState::Due,
                serde_json::to_string(&event)?,
            )
            .await?;
        event_from_record(record)
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
