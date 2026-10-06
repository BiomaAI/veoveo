//! Checked conversion of SQL-selected records into the public Time model.
use super::{
    acquisition_state_from_store, event_state_from_store, release_state_from_store, source_kind,
};
use crate::contract::{
    AuthorityRelease, AuthorityReleaseId, MissionEpoch, OperationalCalendar, TemporalEvent,
    TimeAcquisition, TimeSource, TimeVersion,
};
use crate::persistence::{
    TimeAcquisitionRecord, TimeAuthorityReleaseRecord, TimeCalendarVersionRecord,
    TimeMissionEpochRecord, TimeSourceRecord, TimeTemporalEventRecord, validate_key,
};
use anyhow::Result;
use serde::de::DeserializeOwned;
use surrealdb::types::RecordId;

#[derive(Clone, Copy, Debug)]
enum Entity {
    Source,
    Release,
    Acquisition,
    Calendar,
    Epoch,
    Event,
}

impl Entity {
    fn table(self) -> &'static str {
        match self {
            Self::Source => "time_source",
            Self::Release => "time_authority_release",
            Self::Acquisition => "time_acquisition",
            Self::Calendar => "time_calendar_version",
            Self::Epoch => "time_mission_epoch",
            Self::Event => "time_temporal_event",
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::Source => "time-source-",
            Self::Release => "time-release-",
            Self::Acquisition => "time-acquisition-",
            Self::Calendar => "calendar-",
            Self::Epoch => "epoch-",
            Self::Event => "event-",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid stored Time {entity:?} metadata in {field}; inspect the retained record")]
struct StoredMetadataError {
    entity: Entity,
    field: &'static str,
}

fn invalid(entity: Entity, field: &'static str) -> StoredMetadataError {
    StoredMetadataError { entity, field }
}

fn check(entity: Entity, field: &'static str, valid: bool) -> Result<()> {
    if !valid {
        return Err(invalid(entity, field).into());
    }
    Ok(())
}

fn body<T: DeserializeOwned>(entity: Entity, json: &str) -> Result<T> {
    // Serde's diagnostic may quote a stored value. Public errors name only the field.
    serde_json::from_str(json).map_err(|_| invalid(entity, "canonical_json").into())
}

fn version(entity: Entity, field: &'static str, value: i64) -> Result<TimeVersion> {
    u64::try_from(value)
        .ok()
        .and_then(|value| TimeVersion::new(value).ok())
        .ok_or_else(|| invalid(entity, field).into())
}

fn identity(entity: Entity, key: &str, body_key: &str) -> Result<()> {
    reference(entity, "identity", key, body_key, entity)
}

fn reference(
    entity: Entity,
    field: &'static str,
    key: &str,
    body_key: &str,
    target: Entity,
) -> Result<()> {
    check(
        entity,
        field,
        key == body_key && validate_key("key", key, target.prefix()).is_ok(),
    )
}

fn record_id(entity: Entity, id: &RecordId, key: String) -> Result<()> {
    check(
        entity,
        "record_id",
        *id == RecordId::new(entity.table(), key),
    )
}

pub(super) fn source_from_record(record: TimeSourceRecord) -> Result<TimeSource> {
    let kind = Entity::Source;
    let mut value = crate::TimeSourceValue::from(body::<TimeSource>(kind, &record.canonical_json)?);
    value.record_version = version(kind, "record_version", record.record_version)?;
    identity(kind, &record.source_key, value.source_id.as_str())?;
    record_id(kind, &record.id, record.source_key)?;
    check(kind, "name", value.name == record.name)?;
    check(
        kind,
        "dataset_kind",
        source_kind(value.dataset_kind) == record.dataset_kind,
    )?;
    check(kind, "source_url", value.url == record.source_url)?;
    check(
        kind,
        "expected_content_type",
        value.expected_content_type == record.expected_content_type,
    )?;
    check(kind, "enabled", value.enabled == record.enabled)?;
    Ok(value.build()?)
}

pub(super) fn release_from_record(record: TimeAuthorityReleaseRecord) -> Result<AuthorityRelease> {
    let kind = Entity::Release;
    let mut value =
        crate::AuthorityReleaseValue::from(body::<AuthorityRelease>(kind, &record.canonical_json)?);
    value.record_version = version(kind, "record_version", record.record_version)?;
    identity(kind, &record.release_key, value.release_id.as_str())?;
    record_id(kind, &record.id, record.release_key)?;
    reference(
        kind,
        "source_key",
        &record.source_key,
        value.source_id.as_str(),
        Entity::Source,
    )?;
    check(
        kind,
        "dataset_kind",
        source_kind(value.dataset_kind) == record.dataset_kind,
    )?;
    check(
        kind,
        "version_label",
        value.version_label == record.version_label,
    )?;
    check(kind, "source_url", value.source_url == record.source_url)?;
    check(
        kind,
        "source_digest_sha256",
        value.source_digest_sha256.as_hex() == record.source_digest_sha256,
    )?;
    check(
        kind,
        "artifact_path",
        value.artifact_path == record.artifact_path,
    )?;
    check(
        kind,
        "retrieved_at",
        value.retrieved_at == record.retrieved_at,
    )?;
    check(
        kind,
        "validated_at",
        value.validated_at == record.validated_at,
    )?;
    // Retirement changes these columns without rewriting the historical JSON body.
    value.state = release_state_from_store(record.state);
    Ok(value.build()?)
}

pub(super) fn acquisition_from_record(record: TimeAcquisitionRecord) -> Result<TimeAcquisition> {
    let kind = Entity::Acquisition;
    let mut value = body::<TimeAcquisition>(kind, &record.canonical_json)?;
    value.record_version = version(kind, "record_version", record.record_version)?;
    identity(kind, &record.acquisition_key, value.acquisition_id.as_str())?;
    record_id(kind, &record.id, record.acquisition_key)?;
    reference(
        kind,
        "source_key",
        &record.source_key,
        value.source_id.as_str(),
        Entity::Source,
    )?;
    check(
        kind,
        "expected_source_digest_sha256",
        value
            .expected_source_digest_sha256
            .as_ref()
            .map(|digest| digest.as_hex())
            == record.expected_source_digest_sha256.as_deref(),
    )?;
    value.status = acquisition_state_from_store(record.status);
    value.phase = record.phase;
    value.staged_release_id = record
        .staged_release_key
        .map(|key| -> Result<AuthorityReleaseId> {
            check(
                kind,
                "staged_release_key",
                validate_key("key", &key, Entity::Release.prefix()).is_ok(),
            )?;
            AuthorityReleaseId::parse(key).map_err(|_| invalid(kind, "staged_release_key").into())
        })
        .transpose()?;
    value.updated_at = record.updated_at;
    veoveo_types::Check::check(&value)?;
    Ok(value)
}

pub(super) fn calendar_from_record(
    record: TimeCalendarVersionRecord,
) -> Result<OperationalCalendar> {
    let kind = Entity::Calendar;
    let value: OperationalCalendar = body(kind, &record.canonical_json)?;
    identity(kind, &record.calendar_key, value.calendar_id.as_str())?;
    let version = version(kind, "calendar_version", record.calendar_version)?;
    check(kind, "calendar_version", value.version == version)?;
    record_id(
        kind,
        &record.id,
        format!("{}:{}", record.calendar_key, version.get()),
    )?;
    check(kind, "name", value.name == record.name)?;
    check(kind, "zone_id", value.zone_id == record.zone_id)?;
    Ok(value)
}

pub(super) fn epoch_from_record(record: TimeMissionEpochRecord) -> Result<MissionEpoch> {
    let kind = Entity::Epoch;
    let value: MissionEpoch = body(kind, &record.canonical_json)?;
    identity(kind, &record.epoch_key, value.epoch_id.as_str())?;
    let version = version(kind, "epoch_version", record.epoch_version)?;
    check(kind, "epoch_version", value.version == version)?;
    record_id(
        kind,
        &record.id,
        format!("{}:{}", record.epoch_key, version.get()),
    )?;
    check(kind, "name", value.name == record.name)?;
    check(
        kind,
        "tai_seconds_since_1970",
        value.instant.tai_seconds_since_1970 == record.tai_seconds_since_1970,
    )?;
    check(
        kind,
        "nanosecond",
        i64::from(value.instant.nanosecond.get()) == record.nanosecond,
    )?;
    Ok(value)
}

pub(super) fn event_from_record(record: TimeTemporalEventRecord) -> Result<TemporalEvent> {
    let kind = Entity::Event;
    let mut value = body::<TemporalEvent>(kind, &record.canonical_json)?;
    value.record_version = version(kind, "record_version", record.record_version)?;
    identity(kind, &record.event_key, value.event_id.as_str())?;
    record_id(kind, &record.id, record.event_key)?;
    check(kind, "name", value.name == record.name)?;
    check(
        kind,
        "due_tai_seconds_since_1970",
        value.due.tai_seconds_since_1970 == record.due_tai_seconds_since_1970,
    )?;
    check(
        kind,
        "due_nanosecond",
        i64::from(value.due.nanosecond.get()) == record.due_nanosecond,
    )?;
    value.state = event_state_from_store(record.state);
    Ok(value)
}
