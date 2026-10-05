use super::*;

impl TimePersistence {
    pub(crate) async fn create_time_calendar_version(
        &self,
        draft: TimeCalendarVersionDraft,
    ) -> Result<TimeCalendarVersionRecord, PersistenceError> {
        validate_key("calendar_key", &draft.calendar_key, "calendar-")?;
        validate_positive("calendar_version", draft.calendar_version.get() as i64)?;
        validate_text("name", &draft.name, 256)?;
        validate_zone_id(&draft.zone_id)?;
        if draft.canonical_json.len() > 64 * 1024 {
            return Err(invalid("calendar", "must fit the 64 KiB resource limit"));
        }
        validate_json(&draft.canonical_json)?;
        let now = Utc::now();
        let record_key = format!("{}:{}", draft.calendar_key, draft.calendar_version.get());
        let content = TimeCalendarVersionContent {
            provenance: TimeProvenanceRecord::new(&draft.identity, &draft.work_context),
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            calendar_key: draft.calendar_key.to_string(),
            calendar_version: draft.calendar_version.get() as i64,
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
        .ok_or(PersistenceError::MissingRecord {
            operation: "time calendar creation readback",
        })
    }

    pub(crate) async fn time_calendar_version(
        &self,
        tenant_id: TenantId,
        calendar_key: &CalendarId,
        version: TimeVersion,
    ) -> Result<Option<TimeCalendarVersionRecord>, PersistenceError> {
        validate_key("calendar_key", calendar_key, "calendar-")?;

        select_one(
            self,
            time_record(
                "time_calendar_version",
                format!("{calendar_key}:{}", version.get()),
            ),
            tenant_id,
        )
        .await
    }

    pub(crate) async fn create_time_mission_epoch(
        &self,
        draft: TimeMissionEpochDraft,
    ) -> Result<TimeMissionEpochRecord, PersistenceError> {
        validate_key("epoch_key", &draft.epoch_key, "epoch-")?;
        validate_text("name", &draft.name, 256)?;
        validate_positive("epoch_version", draft.epoch_version.get() as i64)?;
        validate_json(&draft.canonical_json)?;
        let now = Utc::now();
        let record_key = format!("{}:{}", draft.epoch_key, draft.epoch_version.get());
        let content = TimeMissionEpochContent {
            provenance: TimeProvenanceRecord::new(&draft.identity, &draft.work_context),
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            epoch_key: draft.epoch_key.to_string(),
            name: draft.name,
            epoch_version: draft.epoch_version.get() as i64,
            tai_seconds_since_1970: draft.tai_seconds_since_1970,
            nanosecond: i64::from(draft.nanosecond.get()),
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
        .ok_or(PersistenceError::MissingRecord {
            operation: "time mission epoch creation readback",
        })
    }

    pub(crate) async fn time_mission_epoch(
        &self,
        tenant_id: TenantId,
        epoch_key: &MissionEpochId,
        version: TimeVersion,
    ) -> Result<Option<TimeMissionEpochRecord>, PersistenceError> {
        validate_key("epoch_key", epoch_key, "epoch-")?;

        select_one(
            self,
            time_record(
                "time_mission_epoch",
                format!("{epoch_key}:{}", version.get()),
            ),
            tenant_id,
        )
        .await
    }

    /// Read the latest version of one tenant epoch without loading other epochs.
    pub(crate) async fn latest_time_mission_epoch(
        &self,
        tenant_id: TenantId,
        epoch_key: &MissionEpochId,
    ) -> Result<Option<TimeMissionEpochRecord>, PersistenceError> {
        validate_key("epoch_key", epoch_key, "epoch-")?;
        let mut response = self
            .client()
            .query(include_str!("queries/latest_epoch.surql"))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("key", epoch_key.to_string()))
            .await?
            .check()?;
        let rows: Vec<TimeMissionEpochRecord> = response.take(0)?;
        Ok(rows.into_iter().next())
    }
}
