use super::*;

impl TimePersistence {
    pub(crate) async fn create_time_acquisition(
        &self,
        draft: TimeAcquisitionDraft,
    ) -> Result<TimeAcquisitionRecord, PersistenceError> {
        validate_acquisition(&draft)?;
        if let Some(existing) = self
            .time_acquisition_for_idempotency(
                draft.identity.tenant_id,
                draft.identity.principal_id,
                &draft.idempotency_key,
            )
            .await?
        {
            if existing.source_key == draft.source_key.as_str()
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
            acquisition_key: draft.acquisition_key.to_string(),
            source_key: draft.source_key.to_string(),
            expected_source_digest_sha256: draft.expected_source_digest_sha256,
            idempotency_key: draft.idempotency_key,
            status: draft.status,
            phase: draft.phase,
            staged_release_key: draft.staged_release_key.map(String::from),
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
            .ok_or(PersistenceError::MissingRecord {
                operation: "time acquisition creation readback",
            })
    }

    pub(crate) async fn time_acquisition(
        &self,
        tenant_id: TenantId,
        key: &TimeAcquisitionId,
    ) -> Result<Option<TimeAcquisitionRecord>, PersistenceError> {
        validate_key("acquisition_key", key, "time-acquisition-")?;
        select_one(self, time_record("time_acquisition", key), tenant_id).await
    }

    pub(crate) async fn list_time_acquisitions(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeAcquisitionRecord>, PersistenceError> {
        select_list(
            self,
            "SELECT * FROM time_acquisition WHERE tenant = $tenant ORDER BY created_at DESC;",
            tenant_id,
        )
        .await
    }

    pub(crate) async fn time_acquisition_for_release(
        &self,
        tenant_id: TenantId,
        release_key: &AuthorityReleaseId,
    ) -> Result<Option<TimeAcquisitionRecord>, PersistenceError> {
        validate_key("staged_release_key", release_key, "time-release-")?;
        let mut response = self
            .client()
            .query("SELECT * FROM time_acquisition WHERE tenant = $tenant AND staged_release_key = $release_key LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("release_key", release_key.to_string()))
            .await?
            .check()?;
        let records: Vec<TimeAcquisitionRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    pub(crate) async fn update_time_acquisition(
        &self,
        update: TimeAcquisitionUpdate,
    ) -> Result<TimeAcquisitionRecord, PersistenceError> {
        validate_key(
            "acquisition_key",
            &update.acquisition_key,
            "time-acquisition-",
        )?;
        validate_text("phase", &update.phase, 128)?;
        validate_json(&update.canonical_json)?;
        if let Some(release) = &update.staged_release_key {
            validate_key("staged_release_key", release, "time-release-")?;
        }
        let mut response = self.client().query("UPDATE $record MERGE { status: $status, phase: $phase, staged_release_key: $staged, canonical_json: $canonical_json, record_version: $next, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
            .bind(("record", time_record("time_acquisition", &update.acquisition_key))).bind(("tenant", update.tenant_id.record_id())).bind(("status", update.status)).bind(("phase", update.phase)).bind(("staged", update.staged_release_key.map(String::from))).bind(("canonical_json", update.canonical_json)).bind(("expected", update.expected_record_version)).bind(("next", update.expected_record_version + 1)).await?.check()?;
        response
            .take::<Option<TimeAcquisitionRecord>>(0)?
            .ok_or_else(|| conflict("acquisition", update.acquisition_key.to_string()))
    }

    pub(crate) async fn time_acquisition_for_idempotency(
        &self,
        tenant_id: TenantId,
        owner: veoveo_platform_store::PrincipalId,
        key: &str,
    ) -> Result<Option<TimeAcquisitionRecord>, PersistenceError> {
        validate_text("idempotency_key", key, 256)?;
        let mut response = self.client().query("SELECT * FROM time_acquisition WHERE tenant = $tenant AND owner = $owner AND idempotency_key = $key LIMIT 1;").bind(("tenant", tenant_id.record_id())).bind(("owner", owner.record_id())).bind(("key", key.to_owned())).await?.check()?;
        let records: Vec<TimeAcquisitionRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }
}
