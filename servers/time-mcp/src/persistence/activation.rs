//! Optimistic authority publication rechecks relationships in its transaction.
use super::*;

const ACTIVATE: &str = r#"
BEGIN TRANSACTION;
LET $release_updated = (UPDATE ONLY $release MERGE {
    state: 'active', canonical_json: $canonical_json,
    record_version: $next_release, updated_at: time::now()
} WHERE tenant = $tenant AND dataset_kind = $dataset_kind
    AND release_key = $release_key AND state = 'staged'
    AND record_version = $expected_release RETURN AFTER);
IF $release_updated = NONE { THROW 'time_authority_release_conflict'; };
IF $expected_pointer = 0 {
    CREATE ONLY $active CONTENT {
        tenant: $tenant, dataset_kind: $dataset_kind, release_key: $release_key,
        previous_release_key: NONE, activated_by: $owner,
        activated_at: time::now(), record_version: 1
    } RETURN NONE;
} ELSE {
    LET $pointer_updated = (UPDATE ONLY $active MERGE {
        release_key: $release_key, previous_release_key: $previous,
        activated_by: $owner, activated_at: time::now(), record_version: $next_pointer
    } WHERE tenant = $tenant AND dataset_kind = $dataset_kind
        AND release_key = $previous AND previous_release_key = $expected_previous
        AND record_version = $expected_pointer RETURN AFTER);
    IF $pointer_updated = NONE { THROW 'time_active_authority_conflict'; };
    LET $previous_retired = (UPDATE ONLY $previous_release MERGE {
        state: 'retired', record_version: record_version + 1, updated_at: time::now()
    } WHERE tenant = $tenant AND dataset_kind = $dataset_kind
        AND release_key = $previous AND state = 'active'
        AND record_version = $previous_version
        AND record_version > 0 AND record_version < $max_version RETURN AFTER);
    IF $previous_retired = NONE { THROW 'time_previous_authority_conflict'; };
};
COMMIT TRANSACTION;
"#;

impl TimePersistence {
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
        if release.record_version != expected_release_version.get() as i64
            || release.state != TimeAuthorityReleaseState::Staged
            || release.release_key != release_key.as_str()
        {
            return Err(conflict("authority release", release_key.to_string()));
        }
        let kind = release.dataset_kind;
        let pointer = self.active_time_authority(identity.tenant_id, kind).await?;
        if pointer.as_ref().map_or(TimeWriteGuard::Absent, |record| {
            TimeWriteGuard::Existing(record.record_version)
        }) != expected_pointer_version
        {
            return Err(conflict(
                "active authority",
                dataset_kind_key(kind).to_owned(),
            ));
        }
        let active_key = format!("{}:{}", identity.tenant_id, dataset_kind_key(kind));
        let previous = pointer
            .as_ref()
            .map(|record| record.release_key.to_string());
        let previous_version = pointer.as_ref().map(|record| record.release.record_version);
        self.client()
            .query(ACTIVATE)
            .bind(("active", time_record("time_active_authority", active_key)))
            .bind((
                "previous_release",
                pointer
                    .as_ref()
                    .map(|record| time_record("time_authority_release", &record.release_key)),
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
                "expected_previous",
                pointer.as_ref().and_then(|record| {
                    record
                        .previous_release_key
                        .as_ref()
                        .map(ToString::to_string)
                }),
            ))
            .bind(("previous_version", previous_version))
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
}
