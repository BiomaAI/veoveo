use super::*;

impl TimePersistence {
    pub(crate) async fn create_time_source(
        &self,
        draft: TimeSourceDraft,
    ) -> Result<TimeSourceRecord, PersistenceError> {
        validate_source(&draft)?;
        let now = Utc::now();
        let content = TimeSourceContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            source_key: draft.source_key.to_string(),
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
            .ok_or(PersistenceError::MissingRecord {
                operation: "time source creation readback",
            })
    }

    pub(crate) async fn replace_time_source(
        &self,
        draft: TimeSourceDraft,
        expected_record_version: TimeVersion,
    ) -> Result<TimeSourceRecord, PersistenceError> {
        validate_source(&draft)?;
        let mut response = self.client().query("UPDATE $record MERGE { name: $name, dataset_kind: $dataset_kind, source_url: $source_url, expected_content_type: $expected_content_type, enabled: $enabled, canonical_json: $canonical_json, record_version: $next, updated_at: time::now() } WHERE tenant = $tenant AND record_version = $expected RETURN AFTER;")
            .bind(("record", time_record("time_source", &draft.source_key)))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("name", draft.name))
            .bind(("dataset_kind", draft.dataset_kind))
            .bind(("source_url", draft.source_url))
            .bind(("expected_content_type", draft.expected_content_type))
            .bind(("enabled", draft.enabled))
            .bind(("canonical_json", draft.canonical_json))
            .bind(("expected", expected_record_version.get() as i64))
            .bind(("next", expected_record_version.checked_next()?.get() as i64)).await?.check()?;
        response
            .take::<Option<TimeSourceRecord>>(0)?
            .ok_or_else(|| conflict("source", draft.source_key.to_string()))
    }

    pub(crate) async fn time_source(
        &self,
        tenant_id: TenantId,
        source_key: &TimeSourceId,
    ) -> Result<Option<TimeSourceRecord>, PersistenceError> {
        validate_key("source_key", source_key, "time-source-")?;
        select_one(self, time_record("time_source", source_key), tenant_id).await
    }

    pub(crate) async fn list_time_sources(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeSourceRecord>, PersistenceError> {
        select_list(
            self,
            "SELECT * FROM time_source WHERE tenant = $tenant ORDER BY name ASC;",
            tenant_id,
        )
        .await
    }

    pub(crate) async fn create_time_authority_release(
        &self,
        draft: TimeAuthorityReleaseDraft,
    ) -> Result<TimeAuthorityReleaseRecord, PersistenceError> {
        validate_release(&draft)?;
        let now = Utc::now();
        let content = TimeAuthorityReleaseContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            release_key: draft.release_key.to_string(),
            source_key: draft.source_key.to_string(),
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
            .ok_or(PersistenceError::MissingRecord {
                operation: "time authority release creation readback",
            })
    }

    pub(crate) async fn time_authority_release(
        &self,
        tenant_id: TenantId,
        release_key: &AuthorityReleaseId,
    ) -> Result<Option<TimeAuthorityReleaseRecord>, PersistenceError> {
        validate_key("release_key", release_key, "time-release-")?;
        select_one(
            self,
            time_record("time_authority_release", release_key),
            tenant_id,
        )
        .await
    }

    pub(crate) async fn list_time_authority_releases(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeAuthorityReleaseRecord>, PersistenceError> {
        select_list(
            self,
            "SELECT * FROM time_authority_release WHERE tenant = $tenant ORDER BY created_at DESC;",
            tenant_id,
        )
        .await
    }

    pub(crate) async fn activate_time_authority_release(
        &self,
        identity: &PlatformIdentity,
        release_key: &AuthorityReleaseId,
        expected_release_version: TimeVersion,
        expected_pointer_version: TimeWriteGuard,
        canonical_json: String,
    ) -> Result<TimeAuthorityReleaseRecord, PersistenceError> {
        validate_key("release_key", release_key, "time-release-")?;
        let next_release = expected_release_version.checked_next()?;
        let next_pointer = expected_pointer_version.next_version()?;
        validate_json(&canonical_json)?;
        let release = self
            .time_authority_release(identity.tenant_id, release_key)
            .await?
            .ok_or_else(|| conflict("authority release", release_key.to_string()))?;
        if release.record_version != expected_release_version.get() as i64 {
            return Err(conflict("authority release", release_key.to_string()));
        }
        let kind = release.dataset_kind;
        let pointer = self.active_time_authority(identity.tenant_id, kind).await?;
        if pointer.as_ref().map_or(0, |record| record.record_version)
            != expected_pointer_version.expected_version() as i64
        {
            return Err(conflict(
                "active authority",
                dataset_kind_key(kind).to_owned(),
            ));
        }
        let active_key = format!("{}:{}", identity.tenant_id, dataset_kind_key(kind));
        let previous = pointer.map(|record| record.release_key);
        let retire_previous = previous
            .as_ref()
            .filter(|previous| previous.as_str() != release_key.as_str())
            .map(|_| "LET $previous_retired = (UPDATE ONLY $previous_release MERGE { state: 'retired', record_version: record_version + 1, updated_at: time::now() } WHERE tenant = $tenant AND record_version > 0 AND record_version < $max_version RETURN AFTER); IF $previous_retired = NONE { THROW 'time_previous_authority_conflict'; };")
            .unwrap_or_default();
        let pointer_statement = if expected_pointer_version == TimeWriteGuard::Absent {
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
                    previous.as_deref().unwrap_or(release_key.as_str()),
                ),
            ))
            .bind((
                "release",
                time_record("time_authority_release", release_key),
            ))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("dataset_kind", kind))
            .bind(("release_key", release_key.to_string()))
            .bind(("previous", previous))
            .bind((
                "expected_pointer",
                expected_pointer_version.expected_version() as i64,
            ))
            .bind(("next_pointer", next_pointer.get() as i64))
            .bind(("max_version", i64::MAX))
            .bind(("expected_release", expected_release_version.get() as i64))
            .bind(("next_release", next_release.get() as i64))
            .bind(("canonical_json", canonical_json))
            .await?
            .check()
            .map_err(|error| {
                if error.to_string().contains("time_")
                    || error.to_string().contains("failed transaction")
                {
                    conflict("authority activation", release_key.to_string())
                } else {
                    PersistenceError::Database(error)
                }
            })?;
        self.time_authority_release(identity.tenant_id, release_key)
            .await?
            .ok_or(PersistenceError::MissingRecord {
                operation: "time authority activation readback",
            })
    }

    pub(crate) async fn active_time_authority(
        &self,
        tenant_id: TenantId,
        kind: TimeDatasetKind,
    ) -> Result<Option<TimeActiveAuthorityRecord>, PersistenceError> {
        let key = format!("{tenant_id}:{}", dataset_kind_key(kind));
        select_one(self, time_record("time_active_authority", &key), tenant_id).await
    }

    pub(crate) async fn list_active_time_authorities(
        &self,
        tenant_id: TenantId,
    ) -> Result<Vec<TimeActiveAuthorityRecord>, PersistenceError> {
        select_list(
            self,
            "SELECT * FROM time_active_authority WHERE tenant = $tenant ORDER BY dataset_kind ASC;",
            tenant_id,
        )
        .await
    }
}
