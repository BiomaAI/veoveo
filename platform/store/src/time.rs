use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use url::Url;
use uuid::Uuid;

use crate::{
    PlatformIdentity, PlatformStore, StoreError, TenantId, TimeAcquisitionRecord,
    TimeAcquisitionState, TimeActiveAuthorityRecord, TimeAuthorityReleaseRecord,
    TimeAuthorityReleaseState, TimeCalendarState, TimeCalendarVersionRecord, TimeClockPolicyRecord,
    TimeDatasetKind, TimeMissionEpochRecord, TimeSourceRecord, TimeTemporalEventRecord,
    TimeTemporalEventState,
};

const MAX_CANONICAL_JSON_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct TimeSourceDraft {
    pub identity: PlatformIdentity,
    pub source_key: String,
    pub name: String,
    pub dataset_kind: TimeDatasetKind,
    pub source_url: String,
    pub expected_content_type: String,
    pub enabled: bool,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct TimeAuthorityReleaseDraft {
    pub identity: PlatformIdentity,
    pub release_key: String,
    pub source_key: String,
    pub dataset_kind: TimeDatasetKind,
    pub state: TimeAuthorityReleaseState,
    pub version_label: String,
    pub source_url: String,
    pub source_digest_sha256: String,
    pub artifact_path: String,
    pub retrieved_at: DateTime<Utc>,
    pub validated_at: DateTime<Utc>,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct TimeAcquisitionDraft {
    pub identity: PlatformIdentity,
    pub acquisition_key: String,
    pub source_key: String,
    pub expected_source_digest_sha256: Option<String>,
    pub idempotency_key: String,
    pub status: TimeAcquisitionState,
    pub phase: String,
    pub staged_release_key: Option<String>,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct TimeCalendarVersionDraft {
    pub identity: PlatformIdentity,
    pub calendar_key: String,
    pub calendar_version: i64,
    pub name: String,
    pub zone_id: String,
    pub state: TimeCalendarState,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct TimeMissionEpochDraft {
    pub identity: PlatformIdentity,
    pub epoch_key: String,
    pub name: String,
    pub epoch_version: i64,
    pub tai_seconds_since_1970: i64,
    pub nanosecond: i64,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct TimeTemporalEventDraft {
    pub identity: PlatformIdentity,
    pub event_key: String,
    pub name: String,
    pub state: TimeTemporalEventState,
    pub due_tai_seconds_since_1970: i64,
    pub due_nanosecond: i64,
    pub idempotency_key: String,
    pub canonical_json: String,
}

#[derive(Clone, Debug)]
pub struct TimeClockPolicyDraft {
    pub identity: PlatformIdentity,
    pub maximum_error_nanoseconds: i64,
    pub maximum_stratum: i64,
    pub minimum_source_diversity: i64,
    pub maximum_holdover_seconds: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct TimeSourceContent {
    tenant: RecordId,
    owner: RecordId,
    source_key: String,
    name: String,
    dataset_kind: TimeDatasetKind,
    source_url: String,
    expected_content_type: String,
    enabled: bool,
    canonical_json: String,
    record_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct TimeAuthorityReleaseContent {
    tenant: RecordId,
    owner: RecordId,
    release_key: String,
    source_key: String,
    dataset_kind: TimeDatasetKind,
    state: TimeAuthorityReleaseState,
    version_label: String,
    source_url: String,
    source_digest_sha256: String,
    artifact_path: String,
    retrieved_at: DateTime<Utc>,
    validated_at: DateTime<Utc>,
    canonical_json: String,
    record_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct TimeAcquisitionContent {
    tenant: RecordId,
    owner: RecordId,
    acquisition_key: String,
    source_key: String,
    expected_source_digest_sha256: Option<String>,
    idempotency_key: String,
    status: TimeAcquisitionState,
    phase: String,
    staged_release_key: Option<String>,
    canonical_json: String,
    record_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct TimeCalendarVersionContent {
    tenant: RecordId,
    owner: RecordId,
    calendar_key: String,
    calendar_version: i64,
    name: String,
    zone_id: String,
    state: TimeCalendarState,
    canonical_json: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct TimeMissionEpochContent {
    tenant: RecordId,
    owner: RecordId,
    epoch_key: String,
    name: String,
    epoch_version: i64,
    tai_seconds_since_1970: i64,
    nanosecond: i64,
    canonical_json: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct TimeTemporalEventContent {
    tenant: RecordId,
    owner: RecordId,
    event_key: String,
    name: String,
    state: TimeTemporalEventState,
    due_tai_seconds_since_1970: i64,
    due_nanosecond: i64,
    idempotency_key: String,
    canonical_json: String,
    record_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct TimeClockPolicyContent {
    tenant: RecordId,
    owner: RecordId,
    maximum_error_nanoseconds: i64,
    maximum_stratum: i64,
    minimum_source_diversity: i64,
    maximum_holdover_seconds: i64,
    record_version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl PlatformStore {
    pub async fn create_time_source(
        &self,
        draft: TimeSourceDraft,
    ) -> Result<TimeSourceRecord, StoreError> {
        validate_source(&draft)?;
        let now = Utc::now();
        let content = TimeSourceContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            source_key: draft.source_key.clone(),
            name: draft.name,
            dataset_kind: draft.dataset_kind,
            source_url: draft.source_url,
            expected_content_type: draft.expected_content_type,
            enabled: draft.enabled,
            canonical_json: draft.canonical_json,
            record_version: 1,
            created_at: now,
            updated_at: now,
        };
        create_only(self, time_record("time_source", &draft.source_key), content).await?;
        self.time_source(draft.identity.tenant_id, &draft.source_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "time source creation readback",
            })
    }

    pub async fn replace_time_source(
        &self,
        draft: TimeSourceDraft,
        expected_record_version: i64,
    ) -> Result<TimeSourceRecord, StoreError> {
        validate_source(&draft)?;
        validate_positive("expected_record_version", expected_record_version)?;
        let mut response = self.client().query("UPDATE $record MERGE { name: $name, dataset_kind: $dataset_kind, source_url: $source_url, expected_content_type: $expected_content_type, enabled: $enabled, canonical_json: $canonical_json, record_version: $next, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
            .bind(("record", time_record("time_source", &draft.source_key)))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("name", draft.name))
            .bind(("dataset_kind", draft.dataset_kind))
            .bind(("source_url", draft.source_url))
            .bind(("expected_content_type", draft.expected_content_type))
            .bind(("enabled", draft.enabled))
            .bind(("canonical_json", draft.canonical_json))
            .bind(("expected", expected_record_version))
            .bind(("next", expected_record_version + 1)).await?.check()?;
        response
            .take::<Option<TimeSourceRecord>>(0)?
            .ok_or_else(|| conflict("source", draft.source_key))
    }

    pub async fn time_source(
        &self,
        tenant_id: TenantId,
        source_key: &str,
    ) -> Result<Option<TimeSourceRecord>, StoreError> {
        validate_key("source_key", source_key, "time-source-")?;
        select_one(self, time_record("time_source", source_key), tenant_id).await
    }

    pub async fn list_time_sources(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeSourceRecord>, StoreError> {
        select_list(
            self,
            "SELECT * FROM time_source WHERE tenant = $tenant ORDER BY name ASC;",
            tenant_id,
        )
        .await
    }

    pub async fn create_time_authority_release(
        &self,
        draft: TimeAuthorityReleaseDraft,
    ) -> Result<TimeAuthorityReleaseRecord, StoreError> {
        validate_release(&draft)?;
        let now = Utc::now();
        let content = TimeAuthorityReleaseContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            release_key: draft.release_key.clone(),
            source_key: draft.source_key,
            dataset_kind: draft.dataset_kind,
            state: draft.state,
            version_label: draft.version_label,
            source_url: draft.source_url,
            source_digest_sha256: draft.source_digest_sha256,
            artifact_path: draft.artifact_path,
            retrieved_at: draft.retrieved_at,
            validated_at: draft.validated_at,
            canonical_json: draft.canonical_json,
            record_version: 1,
            created_at: now,
            updated_at: now,
        };
        create_only(
            self,
            time_record("time_authority_release", &draft.release_key),
            content,
        )
        .await?;
        self.time_authority_release(draft.identity.tenant_id, &draft.release_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "time authority release creation readback",
            })
    }

    pub async fn time_authority_release(
        &self,
        tenant_id: TenantId,
        release_key: &str,
    ) -> Result<Option<TimeAuthorityReleaseRecord>, StoreError> {
        validate_key("release_key", release_key, "time-release-")?;
        select_one(
            self,
            time_record("time_authority_release", release_key),
            tenant_id,
        )
        .await
    }

    pub async fn list_time_authority_releases(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeAuthorityReleaseRecord>, StoreError> {
        select_list(
            self,
            "SELECT * FROM time_authority_release WHERE tenant = $tenant ORDER BY created_at DESC;",
            tenant_id,
        )
        .await
    }

    pub async fn activate_time_authority_release(
        &self,
        identity: &PlatformIdentity,
        release_key: &str,
        expected_release_version: i64,
        expected_pointer_version: i64,
        canonical_json: String,
    ) -> Result<TimeAuthorityReleaseRecord, StoreError> {
        validate_key("release_key", release_key, "time-release-")?;
        validate_positive("expected_release_version", expected_release_version)?;
        validate_json(&canonical_json)?;
        let release = self
            .time_authority_release(identity.tenant_id, release_key)
            .await?
            .ok_or_else(|| conflict("authority release", release_key.to_owned()))?;
        if release.record_version != expected_release_version {
            return Err(conflict("authority release", release_key.to_owned()));
        }
        let kind = release.dataset_kind;
        let pointer = self.active_time_authority(identity.tenant_id, kind).await?;
        if pointer.as_ref().map_or(0, |record| record.record_version) != expected_pointer_version {
            return Err(conflict(
                "active authority",
                dataset_kind_key(kind).to_owned(),
            ));
        }
        let active_key = format!("{}:{}", identity.tenant_id, dataset_kind_key(kind));
        let previous = pointer.map(|record| record.release_key);
        let retire_previous = previous
            .as_ref()
            .filter(|previous| previous.as_str() != release_key)
            .map(|_| "UPDATE ONLY $previous_release MERGE { state: 'retired', record_version: record_version + 1, updated_at: time::now() } WHERE tenant = $tenant;")
            .unwrap_or_default();
        let pointer_statement = if expected_pointer_version == 0 {
            "CREATE ONLY $active CONTENT { tenant: $tenant, dataset_kind: $dataset_kind, release_key: $release_key, previous_release_key: $previous, activated_by: $owner, activated_at: time::now(), record_version: 1 } RETURN NONE;"
        } else {
            "LET $pointer_updated = (UPDATE ONLY $active MERGE { release_key: $release_key, previous_release_key: $previous, activated_by: $owner, activated_at: time::now(), record_version: $next_pointer } WHERE tenant = $tenant AND record_version = $expected_pointer RETURN AFTER); IF $pointer_updated = NONE { THROW 'time_active_authority_conflict'; };"
        };
        let query = format!(
            "BEGIN TRANSACTION; LET $release_updated = (UPDATE ONLY $release MERGE {{ state: 'active', canonical_json: $canonical_json, record_version: $next_release, updated_at: time::now() }} WHERE tenant = $tenant AND record_version = $expected_release RETURN AFTER); IF $release_updated = NONE {{ THROW 'time_authority_release_conflict'; }}; {pointer_statement} {retire_previous} COMMIT TRANSACTION;"
        );
        self.client()
            .query(query)
            .bind(("active", time_record("time_active_authority", &active_key)))
            .bind((
                "previous_release",
                time_record(
                    "time_authority_release",
                    previous.as_deref().unwrap_or(release_key),
                ),
            ))
            .bind((
                "release",
                time_record("time_authority_release", release_key),
            ))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("dataset_kind", kind))
            .bind(("release_key", release_key.to_owned()))
            .bind(("previous", previous))
            .bind(("expected_pointer", expected_pointer_version))
            .bind(("next_pointer", expected_pointer_version + 1))
            .bind(("expected_release", expected_release_version))
            .bind(("next_release", expected_release_version + 1))
            .bind(("canonical_json", canonical_json))
            .await?
            .check()
            .map_err(|error| {
                if error.to_string().contains("time_")
                    || error.to_string().contains("failed transaction")
                {
                    conflict("authority activation", release_key.to_owned())
                } else {
                    StoreError::Database(error)
                }
            })?;
        self.time_authority_release(identity.tenant_id, release_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "time authority activation readback",
            })
    }

    pub async fn active_time_authority(
        &self,
        tenant_id: TenantId,
        kind: TimeDatasetKind,
    ) -> Result<Option<TimeActiveAuthorityRecord>, StoreError> {
        let key = format!("{tenant_id}:{}", dataset_kind_key(kind));
        select_one(self, time_record("time_active_authority", &key), tenant_id).await
    }

    pub async fn list_active_time_authorities(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeActiveAuthorityRecord>, StoreError> {
        select_list(
            self,
            "SELECT * FROM time_active_authority WHERE tenant = $tenant ORDER BY dataset_kind ASC;",
            tenant_id,
        )
        .await
    }

    pub async fn create_time_acquisition(
        &self,
        draft: TimeAcquisitionDraft,
    ) -> Result<TimeAcquisitionRecord, StoreError> {
        validate_acquisition(&draft)?;
        if let Some(existing) = self
            .time_acquisition_for_idempotency(
                draft.identity.tenant_id,
                draft.identity.principal_id.record_id(),
                &draft.idempotency_key,
            )
            .await?
        {
            if existing.source_key == draft.source_key
                && existing.expected_source_digest_sha256 == draft.expected_source_digest_sha256
            {
                return Ok(existing);
            }
            return Err(conflict(
                "acquisition idempotency key",
                draft.idempotency_key,
            ));
        }
        let now = Utc::now();
        let content = TimeAcquisitionContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            acquisition_key: draft.acquisition_key.clone(),
            source_key: draft.source_key,
            expected_source_digest_sha256: draft.expected_source_digest_sha256,
            idempotency_key: draft.idempotency_key,
            status: draft.status,
            phase: draft.phase,
            staged_release_key: draft.staged_release_key,
            canonical_json: draft.canonical_json,
            record_version: 1,
            created_at: now,
            updated_at: now,
        };
        create_only(
            self,
            time_record("time_acquisition", &draft.acquisition_key),
            content,
        )
        .await?;
        self.time_acquisition(draft.identity.tenant_id, &draft.acquisition_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "time acquisition creation readback",
            })
    }

    pub async fn time_acquisition(
        &self,
        tenant_id: TenantId,
        key: &str,
    ) -> Result<Option<TimeAcquisitionRecord>, StoreError> {
        validate_key("acquisition_key", key, "time-acquisition-")?;
        select_one(self, time_record("time_acquisition", key), tenant_id).await
    }

    pub async fn list_time_acquisitions(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeAcquisitionRecord>, StoreError> {
        select_list(
            self,
            "SELECT * FROM time_acquisition WHERE tenant = $tenant ORDER BY created_at DESC;",
            tenant_id,
        )
        .await
    }

    pub async fn update_time_acquisition(
        &self,
        tenant_id: TenantId,
        key: &str,
        expected: i64,
        status: TimeAcquisitionState,
        phase: String,
        staged_release_key: Option<String>,
        canonical_json: String,
    ) -> Result<TimeAcquisitionRecord, StoreError> {
        validate_key("acquisition_key", key, "time-acquisition-")?;
        validate_text("phase", &phase, 128)?;
        validate_json(&canonical_json)?;
        if let Some(release) = &staged_release_key {
            validate_key("staged_release_key", release, "time-release-")?;
        }
        let mut response = self.client().query("UPDATE $record MERGE { status: $status, phase: $phase, staged_release_key: $staged, canonical_json: $canonical_json, record_version: $next, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
            .bind(("record", time_record("time_acquisition", key))).bind(("tenant", tenant_id.record_id())).bind(("status", status)).bind(("phase", phase)).bind(("staged", staged_release_key)).bind(("canonical_json", canonical_json)).bind(("expected", expected)).bind(("next", expected + 1)).await?.check()?;
        response
            .take::<Option<TimeAcquisitionRecord>>(0)?
            .ok_or_else(|| conflict("acquisition", key.to_owned()))
    }

    pub async fn create_time_calendar_version(
        &self,
        draft: TimeCalendarVersionDraft,
    ) -> Result<TimeCalendarVersionRecord, StoreError> {
        validate_key("calendar_key", &draft.calendar_key, "calendar-")?;
        validate_positive("calendar_version", draft.calendar_version)?;
        validate_text("name", &draft.name, 256)?;
        validate_zone_id(&draft.zone_id)?;
        validate_json(&draft.canonical_json)?;
        let now = Utc::now();
        let record_key = format!("{}:{}", draft.calendar_key, draft.calendar_version);
        let content = TimeCalendarVersionContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            calendar_key: draft.calendar_key.clone(),
            calendar_version: draft.calendar_version,
            name: draft.name,
            zone_id: draft.zone_id,
            state: draft.state,
            canonical_json: draft.canonical_json,
            created_at: now,
            updated_at: now,
        };
        create_only(
            self,
            time_record("time_calendar_version", &record_key),
            content,
        )
        .await?;
        self.time_calendar_version(
            draft.identity.tenant_id,
            &draft.calendar_key,
            draft.calendar_version,
        )
        .await?
        .ok_or(StoreError::MissingRecord {
            operation: "time calendar creation readback",
        })
    }

    pub async fn time_calendar_version(
        &self,
        tenant_id: TenantId,
        calendar_key: &str,
        version: i64,
    ) -> Result<Option<TimeCalendarVersionRecord>, StoreError> {
        validate_key("calendar_key", calendar_key, "calendar-")?;
        validate_positive("calendar_version", version)?;
        select_one(
            self,
            time_record(
                "time_calendar_version",
                &format!("{calendar_key}:{version}"),
            ),
            tenant_id,
        )
        .await
    }

    pub async fn list_time_calendar_versions(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeCalendarVersionRecord>, StoreError> {
        select_list(self, "SELECT * FROM time_calendar_version WHERE tenant = $tenant ORDER BY calendar_key ASC, calendar_version DESC;", tenant_id).await
    }

    pub async fn create_time_mission_epoch(
        &self,
        draft: TimeMissionEpochDraft,
    ) -> Result<TimeMissionEpochRecord, StoreError> {
        validate_key("epoch_key", &draft.epoch_key, "epoch-")?;
        validate_text("name", &draft.name, 256)?;
        validate_positive("epoch_version", draft.epoch_version)?;
        validate_nanosecond(draft.nanosecond)?;
        validate_json(&draft.canonical_json)?;
        let now = Utc::now();
        let record_key = format!("{}:{}", draft.epoch_key, draft.epoch_version);
        let content = TimeMissionEpochContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            epoch_key: draft.epoch_key.clone(),
            name: draft.name,
            epoch_version: draft.epoch_version,
            tai_seconds_since_1970: draft.tai_seconds_since_1970,
            nanosecond: draft.nanosecond,
            canonical_json: draft.canonical_json,
            created_at: now,
            updated_at: now,
        };
        create_only(
            self,
            time_record("time_mission_epoch", &record_key),
            content,
        )
        .await?;
        self.time_mission_epoch(
            draft.identity.tenant_id,
            &draft.epoch_key,
            draft.epoch_version,
        )
        .await?
        .ok_or(StoreError::MissingRecord {
            operation: "time mission epoch creation readback",
        })
    }

    pub async fn time_mission_epoch(
        &self,
        tenant_id: TenantId,
        epoch_key: &str,
        version: i64,
    ) -> Result<Option<TimeMissionEpochRecord>, StoreError> {
        validate_key("epoch_key", epoch_key, "epoch-")?;
        validate_positive("epoch_version", version)?;
        select_one(
            self,
            time_record("time_mission_epoch", &format!("{epoch_key}:{version}")),
            tenant_id,
        )
        .await
    }

    pub async fn list_time_mission_epochs(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeMissionEpochRecord>, StoreError> {
        select_list(self, "SELECT * FROM time_mission_epoch WHERE tenant = $tenant ORDER BY epoch_key ASC, epoch_version DESC;", tenant_id).await
    }

    pub async fn create_time_temporal_event(
        &self,
        draft: TimeTemporalEventDraft,
    ) -> Result<TimeTemporalEventRecord, StoreError> {
        validate_event(&draft)?;
        if let Some(existing) = self
            .time_event_for_idempotency(
                draft.identity.tenant_id,
                draft.identity.principal_id.record_id(),
                &draft.idempotency_key,
            )
            .await?
        {
            if existing.name == draft.name
                && existing.due_tai_seconds_since_1970 == draft.due_tai_seconds_since_1970
                && existing.due_nanosecond == draft.due_nanosecond
            {
                return Ok(existing);
            }
            return Err(conflict("event idempotency key", draft.idempotency_key));
        }
        let now = Utc::now();
        let content = TimeTemporalEventContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            event_key: draft.event_key.clone(),
            name: draft.name,
            state: draft.state,
            due_tai_seconds_since_1970: draft.due_tai_seconds_since_1970,
            due_nanosecond: draft.due_nanosecond,
            idempotency_key: draft.idempotency_key,
            canonical_json: draft.canonical_json,
            record_version: 1,
            created_at: now,
            updated_at: now,
        };
        create_only(
            self,
            time_record("time_temporal_event", &draft.event_key),
            content,
        )
        .await?;
        self.time_temporal_event(draft.identity.tenant_id, &draft.event_key)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "time event creation readback",
            })
    }

    pub async fn time_temporal_event(
        &self,
        tenant_id: TenantId,
        event_key: &str,
    ) -> Result<Option<TimeTemporalEventRecord>, StoreError> {
        validate_key("event_key", event_key, "event-")?;
        select_one(
            self,
            time_record("time_temporal_event", event_key),
            tenant_id,
        )
        .await
    }

    pub async fn list_time_temporal_events(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeTemporalEventRecord>, StoreError> {
        select_list(self, "SELECT * FROM time_temporal_event WHERE tenant = $tenant ORDER BY due_tai_seconds_since_1970 ASC, due_nanosecond ASC;", tenant_id).await
    }

    pub async fn transition_time_temporal_event(
        &self,
        tenant_id: TenantId,
        event_key: &str,
        expected: i64,
        state: TimeTemporalEventState,
        canonical_json: String,
    ) -> Result<TimeTemporalEventRecord, StoreError> {
        validate_key("event_key", event_key, "event-")?;
        validate_json(&canonical_json)?;
        let mut response = self.client().query("UPDATE $record MERGE { state: $state, canonical_json: $canonical_json, record_version: $next, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
            .bind(("record", time_record("time_temporal_event", event_key))).bind(("tenant", tenant_id.record_id())).bind(("state", state)).bind(("canonical_json", canonical_json)).bind(("expected", expected)).bind(("next", expected + 1)).await?.check()?;
        response
            .take::<Option<TimeTemporalEventRecord>>(0)?
            .ok_or_else(|| conflict("temporal event", event_key.to_owned()))
    }

    pub async fn due_time_temporal_events(
        &self,
        tenant_id: TenantId,
        tai_seconds: i64,
        nanosecond: i64,
        limit: u32,
    ) -> Result<Vec<TimeTemporalEventRecord>, StoreError> {
        validate_nanosecond(nanosecond)?;
        if !(1..=10_000).contains(&limit) {
            return Err(invalid("limit", "must be in 1..=10000"));
        }
        let mut response = self.client().query("SELECT * FROM time_temporal_event WHERE tenant = $tenant AND state = 'scheduled' AND (due_tai_seconds_since_1970 < $seconds OR (due_tai_seconds_since_1970 = $seconds AND due_nanosecond <= $nanosecond)) ORDER BY due_tai_seconds_since_1970 ASC, due_nanosecond ASC LIMIT $limit;")
            .bind(("tenant", tenant_id.record_id())).bind(("seconds", tai_seconds)).bind(("nanosecond", nanosecond)).bind(("limit", i64::from(limit))).await?.check()?;
        Ok(response.take(0)?)
    }

    pub async fn replace_time_clock_policy(
        &self,
        draft: TimeClockPolicyDraft,
        expected: i64,
    ) -> Result<TimeClockPolicyRecord, StoreError> {
        validate_clock_policy(&draft)?;
        let record = time_record("time_clock_policy", &draft.identity.tenant_id.to_string());
        if expected == 0 {
            let now = Utc::now();
            let content = TimeClockPolicyContent {
                tenant: draft.identity.tenant_id.record_id(),
                owner: draft.identity.principal_id.record_id(),
                maximum_error_nanoseconds: draft.maximum_error_nanoseconds,
                maximum_stratum: draft.maximum_stratum,
                minimum_source_diversity: draft.minimum_source_diversity,
                maximum_holdover_seconds: draft.maximum_holdover_seconds,
                record_version: 1,
                created_at: now,
                updated_at: now,
            };
            create_only(self, record, content).await?;
        } else {
            let mut response = self.client().query("UPDATE $record MERGE { owner: $owner, maximum_error_nanoseconds: $maximum_error, maximum_stratum: $maximum_stratum, minimum_source_diversity: $minimum_diversity, maximum_holdover_seconds: $maximum_holdover, record_version: $next, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
                .bind(("record", record)).bind(("owner", draft.identity.principal_id.record_id())).bind(("tenant", draft.identity.tenant_id.record_id())).bind(("maximum_error", draft.maximum_error_nanoseconds)).bind(("maximum_stratum", draft.maximum_stratum)).bind(("minimum_diversity", draft.minimum_source_diversity)).bind(("maximum_holdover", draft.maximum_holdover_seconds)).bind(("expected", expected)).bind(("next", expected + 1)).await?.check()?;
            if response.take::<Option<TimeClockPolicyRecord>>(0)?.is_none() {
                return Err(conflict(
                    "clock policy",
                    draft.identity.tenant_id.to_string(),
                ));
            }
        }
        self.time_clock_policy(draft.identity.tenant_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "time clock policy readback",
            })
    }

    pub async fn time_clock_policy(
        &self,
        tenant_id: TenantId,
    ) -> Result<Option<TimeClockPolicyRecord>, StoreError> {
        select_one(
            self,
            time_record("time_clock_policy", &tenant_id.to_string()),
            tenant_id,
        )
        .await
    }

    pub async fn time_acquisition_for_idempotency(
        &self,
        tenant_id: TenantId,
        owner: RecordId,
        key: &str,
    ) -> Result<Option<TimeAcquisitionRecord>, StoreError> {
        validate_text("idempotency_key", key, 256)?;
        let mut response = self.client().query("SELECT * FROM time_acquisition WHERE tenant = $tenant AND owner = $owner AND idempotency_key = $key LIMIT 1;").bind(("tenant", tenant_id.record_id())).bind(("owner", owner)).bind(("key", key.to_owned())).await?.check()?;
        let records: Vec<TimeAcquisitionRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    async fn time_event_for_idempotency(
        &self,
        tenant_id: TenantId,
        owner: RecordId,
        key: &str,
    ) -> Result<Option<TimeTemporalEventRecord>, StoreError> {
        validate_text("idempotency_key", key, 256)?;
        let mut response = self.client().query("SELECT * FROM time_temporal_event WHERE tenant = $tenant AND owner = $owner AND idempotency_key = $key LIMIT 1;").bind(("tenant", tenant_id.record_id())).bind(("owner", owner)).bind(("key", key.to_owned())).await?.check()?;
        let records: Vec<TimeTemporalEventRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }
}

async fn create_only<T: SurrealValue>(
    store: &PlatformStore,
    record: RecordId,
    content: T,
) -> Result<(), StoreError> {
    store
        .client()
        .query("CREATE ONLY $record CONTENT $content RETURN NONE;")
        .bind(("record", record))
        .bind(("content", content))
        .await?
        .check()?;
    Ok(())
}

async fn select_one<T>(
    store: &PlatformStore,
    record: RecordId,
    tenant_id: TenantId,
) -> Result<Option<T>, StoreError>
where
    T: for<'de> Deserialize<'de> + SurrealValue,
{
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $record WHERE tenant = $tenant;")
        .bind(("record", record))
        .bind(("tenant", tenant_id.record_id()))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

async fn select_list<T>(
    store: &PlatformStore,
    query: &'static str,
    tenant_id: TenantId,
) -> Result<Vec<T>, StoreError>
where
    T: for<'de> Deserialize<'de> + SurrealValue,
{
    let mut response = store
        .client()
        .query(query)
        .bind(("tenant", tenant_id.record_id()))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

fn validate_source(draft: &TimeSourceDraft) -> Result<(), StoreError> {
    validate_key("source_key", &draft.source_key, "time-source-")?;
    validate_text("name", &draft.name, 256)?;
    validate_https_url("source_url", &draft.source_url)?;
    validate_text("expected_content_type", &draft.expected_content_type, 128)?;
    validate_json(&draft.canonical_json)
}

fn validate_release(draft: &TimeAuthorityReleaseDraft) -> Result<(), StoreError> {
    validate_key("release_key", &draft.release_key, "time-release-")?;
    validate_key("source_key", &draft.source_key, "time-source-")?;
    validate_text("version_label", &draft.version_label, 256)?;
    validate_https_url("source_url", &draft.source_url)?;
    validate_sha256("source_digest_sha256", &draft.source_digest_sha256)?;
    validate_absolute_path("artifact_path", &draft.artifact_path)?;
    if draft.validated_at < draft.retrieved_at {
        return Err(invalid("validated_at", "must not precede retrieval"));
    }
    validate_json(&draft.canonical_json)
}

fn validate_acquisition(draft: &TimeAcquisitionDraft) -> Result<(), StoreError> {
    validate_key(
        "acquisition_key",
        &draft.acquisition_key,
        "time-acquisition-",
    )?;
    validate_key("source_key", &draft.source_key, "time-source-")?;
    validate_text("idempotency_key", &draft.idempotency_key, 256)?;
    validate_text("phase", &draft.phase, 128)?;
    if let Some(release) = &draft.staged_release_key {
        validate_key("staged_release_key", release, "time-release-")?;
    }
    validate_json(&draft.canonical_json)
}

fn validate_event(draft: &TimeTemporalEventDraft) -> Result<(), StoreError> {
    validate_key("event_key", &draft.event_key, "event-")?;
    validate_text("name", &draft.name, 256)?;
    validate_nanosecond(draft.due_nanosecond)?;
    validate_text("idempotency_key", &draft.idempotency_key, 256)?;
    validate_json(&draft.canonical_json)
}

fn validate_clock_policy(draft: &TimeClockPolicyDraft) -> Result<(), StoreError> {
    validate_positive("maximum_error_nanoseconds", draft.maximum_error_nanoseconds)?;
    if !(1..=15).contains(&draft.maximum_stratum) {
        return Err(invalid("maximum_stratum", "must be in 1..=15"));
    }
    validate_positive("minimum_source_diversity", draft.minimum_source_diversity)?;
    validate_positive("maximum_holdover_seconds", draft.maximum_holdover_seconds)
}

fn validate_key(field: &'static str, value: &str, prefix: &'static str) -> Result<(), StoreError> {
    let raw = value
        .strip_prefix(prefix)
        .ok_or_else(|| invalid(field, "must use the canonical prefix followed by a UUIDv7"))?;
    let uuid = Uuid::parse_str(raw)
        .map_err(|_| invalid(field, "must use the canonical prefix followed by a UUIDv7"))?;
    if uuid.get_version_num() != 7 {
        return Err(invalid(
            field,
            "must use the canonical prefix followed by a UUIDv7",
        ));
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), StoreError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(invalid(
            field,
            "must be non-empty, bounded, and contain no control characters",
        ));
    }
    Ok(())
}

fn validate_positive(field: &'static str, value: i64) -> Result<(), StoreError> {
    if value < 1 {
        return Err(invalid(field, "must be positive"));
    }
    Ok(())
}

fn validate_nanosecond(value: i64) -> Result<(), StoreError> {
    if !(0..1_000_000_000).contains(&value) {
        return Err(invalid("nanosecond", "must be in 0..1000000000"));
    }
    Ok(())
}

fn validate_zone_id(value: &str) -> Result<(), StoreError> {
    if value.is_empty()
        || value.len() > 128
        || value.starts_with('/')
        || value.contains("..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+'))
    {
        return Err(invalid("zone_id", "must be a bounded IANA zone identifier"));
    }
    Ok(())
}

fn validate_https_url(field: &'static str, value: &str) -> Result<(), StoreError> {
    let url = Url::parse(value).map_err(|_| invalid(field, "must be an absolute HTTPS URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            field,
            "must be an absolute HTTPS URL without credentials or a fragment",
        ));
    }
    Ok(())
}

fn validate_absolute_path(field: &'static str, value: &str) -> Result<(), StoreError> {
    if !value.starts_with('/') || value.contains("/../") || value.chars().any(char::is_control) {
        return Err(invalid(field, "must be a confined absolute path"));
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), StoreError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(
            field,
            "must contain 64 hexadecimal SHA-256 characters",
        ));
    }
    Ok(())
}

fn validate_json(value: &str) -> Result<(), StoreError> {
    if value.len() > MAX_CANONICAL_JSON_BYTES
        || serde_json::from_str::<serde_json::Value>(value).is_err()
    {
        return Err(invalid("canonical_json", "must be valid bounded JSON"));
    }
    Ok(())
}

fn time_record(table: &'static str, key: &str) -> RecordId {
    RecordId::new(table, key.to_owned())
}

fn dataset_kind_key(kind: TimeDatasetKind) -> &'static str {
    match kind {
        TimeDatasetKind::Tzdb => "tzdb",
        TimeDatasetKind::LeapSeconds => "leap_seconds",
    }
}
fn invalid(field: &'static str, reason: &'static str) -> StoreError {
    StoreError::InvalidTimeField { field, reason }
}
fn conflict(entity: &'static str, key: String) -> StoreError {
    StoreError::TimeRecordConflict { entity, key }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_time_keys_require_uuid_v7() {
        assert!(validate_key("event_key", &format!("event-{}", Uuid::now_v7()), "event-").is_ok());
        assert!(validate_key("event_key", &format!("event-{}", Uuid::new_v4()), "event-").is_err());
    }

    #[test]
    fn authority_sources_require_safe_https_urls() {
        assert!(
            validate_https_url(
                "source_url",
                "https://data.iana.org/time-zones/tzdata-latest.tar.gz"
            )
            .is_ok()
        );
        assert!(validate_https_url("source_url", "http://example.test/tzdb").is_err());
        assert!(validate_https_url("source_url", "https://user@example.test/tzdb").is_err());
    }

    #[test]
    fn clock_policy_has_operational_bounds() {
        let identity = PlatformIdentity {
            tenant_id: TenantId::new(),
            principal_id: crate::PrincipalId::new(),
            tenant_key: "tenant-test".to_owned(),
            principal_key: "principal-test".to_owned(),
        };
        assert!(
            validate_clock_policy(&TimeClockPolicyDraft {
                identity,
                maximum_error_nanoseconds: 1_000_000,
                maximum_stratum: 4,
                minimum_source_diversity: 2,
                maximum_holdover_seconds: 300
            })
            .is_ok()
        );
    }
}
