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
            provenance: TimeProvenanceRecord::new(&draft.identity, &draft.work_context),
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            release_key: draft.release_key.to_string(),
            source_key: draft.source_key.to_string(),
            dataset_kind: draft.dataset_kind,
            state: draft.state,
            version_label: draft.version_label,
            source_url: draft.source_url,
            source_digest_sha256: draft.source_digest_sha256.into(),
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
}
