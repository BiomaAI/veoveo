use super::*;

impl TimePersistence {
    pub(crate) async fn create_time_temporal_event(
        &self,
        draft: TimeTemporalEventDraft,
    ) -> Result<TimeTemporalEventRecord, PersistenceError> {
        validate_event(&draft)?;
        if let Some(existing) = self
            .time_event_for_idempotency(
                draft.identity.tenant_id,
                draft.identity.principal_id,
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
            event_key: draft.event_key.to_string(),
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
        self.time_temporal_event(&draft.identity, &draft.event_key)
            .await?
            .ok_or(PersistenceError::MissingRecord {
                operation: "time event creation readback",
            })
    }

    pub(crate) async fn time_temporal_event(
        &self,
        identity: &PlatformIdentity,
        event_key: &TemporalEventId,
    ) -> Result<Option<TimeTemporalEventRecord>, PersistenceError> {
        validate_key("event_key", event_key, "event-")?;
        let mut response = self
            .client()
            .query("SELECT * FROM ONLY $record WHERE tenant = $tenant AND owner = $owner;")
            .bind(("record", time_record("time_temporal_event", event_key)))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub(crate) async fn transition_time_temporal_event(
        &self,
        identity: &PlatformIdentity,
        event_key: &TemporalEventId,
        expected: i64,
        state: TimeTemporalEventState,
        canonical_json: String,
    ) -> Result<TimeTemporalEventRecord, PersistenceError> {
        validate_key("event_key", event_key, "event-")?;
        validate_json(&canonical_json)?;
        let mut response = self.client().query("UPDATE $record MERGE { state: $state, canonical_json: $canonical_json, record_version: $next, updated_at: time::now() } WHERE tenant = $tenant AND owner = $owner AND record_version = $expected RETURN AFTER;")
            .bind(("record", time_record("time_temporal_event", event_key))).bind(("tenant", identity.tenant_id.record_id())).bind(("owner", identity.principal_id.record_id())).bind(("state", state)).bind(("canonical_json", canonical_json)).bind(("expected", expected)).bind(("next", expected + 1)).await?.check()?;
        response
            .take::<Option<TimeTemporalEventRecord>>(0)?
            .ok_or_else(|| conflict("temporal event", event_key.to_string()))
    }

    async fn time_event_for_idempotency(
        &self,
        tenant_id: TenantId,
        owner: veoveo_platform_store::PrincipalId,
        key: &str,
    ) -> Result<Option<TimeTemporalEventRecord>, PersistenceError> {
        validate_text("idempotency_key", key, 256)?;
        let mut response = self.client().query("SELECT * FROM time_temporal_event WHERE tenant = $tenant AND owner = $owner AND idempotency_key = $key LIMIT 1;").bind(("tenant", tenant_id.record_id())).bind(("owner", owner.record_id())).bind(("key", key.to_owned())).await?.check()?;
        let records: Vec<TimeTemporalEventRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }
}
