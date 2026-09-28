//! Optimistic authority publication rechecks relationships in its transaction.
use super::*;

const ACTIVATE: &str = r#"
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

/// Only the catalog constructs a write from a checked candidate and active snapshot.
pub(crate) struct AuthorityActivation {
    pub(crate) candidate: TimeAuthorityReleaseRecord,
    pub(crate) snapshot: ActiveAuthoritySnapshot,
    pub(crate) expected_release: TimeVersion,
    pub(crate) expected_pointer: TimeWriteGuard,
    pub(crate) canonical_json: String,
}

const FENCE: &str = r#"
BEGIN TRANSACTION;
UPSERT ONLY $fence SET tenant = $tenant, token = $activation_token RETURN NONE;
"#;

const CHECK_SNAPSHOT: &str = r#"
IF array::len($observed) != array::len($expected_authorities) {
    THROW 'time_authority_pair_conflict';
};
FOR $index IN 0..array::len($observed) {
    LET $current = $observed[$index];
    LET $expected = $expected_authorities[$index];
    -- Optional history is absent in stored objects and NONE in driver values.
    -- Compare its value, and every other pointer field, without object-key ambiguity.
    IF $current.dataset_kind != $expected.dataset_kind
        OR $current.release != $expected.release
        OR $current.pointer.id != $expected.pointer.id
        OR $current.pointer.tenant != $expected.pointer.tenant
        OR $current.pointer.dataset_kind != $expected.pointer.dataset_kind
        OR $current.pointer.release_key != $expected.pointer.release_key
        OR $current.pointer.previous_release_key != $expected.pointer.previous_release_key
        OR $current.pointer.activated_by != $expected.pointer.activated_by
        OR $current.pointer.activated_at != $expected.pointer.activated_at
        OR $current.pointer.record_version != $expected.pointer.record_version {
        THROW 'time_authority_pair_conflict';
    };
};
LET $candidate = (SELECT * FROM ONLY $release WHERE tenant = $tenant);
IF $candidate != $expected_candidate { THROW 'time_authority_candidate_conflict'; };
"#;

impl TimePersistence {
    pub(crate) async fn commit_time_authority_release(
        &self,
        identity: &PlatformIdentity,
        activation: AuthorityActivation,
    ) -> Result<TimeAuthorityReleaseRecord, PersistenceError> {
        let AuthorityActivation {
            candidate: release,
            snapshot,
            expected_release: expected_release_version,
            expected_pointer: expected_pointer_version,
            canonical_json,
        } = activation;
        if snapshot.tenant != identity.tenant_id {
            return Err(invalid(
                "activation.tenant",
                "snapshot belongs to another tenant",
            ));
        }
        let release_key = AuthorityReleaseId::new(&release.release_key)
            .map_err(|_| invalid("release_key", "invalid release identity"))?;
        validate_key("release_key", &release_key, "time-release-")?;
        let next_release = expected_release_version.checked_next()?;
        let next_pointer = expected_pointer_version.next_version()?;
        validate_json(&canonical_json)?;
        if release.record_version != expected_release_version.get() as i64
            || release.state != TimeAuthorityReleaseState::Staged
            || release.tenant != identity.tenant_id.record_id()
            || release.id != time_record("time_authority_release", &release_key)
        {
            return Err(conflict("authority release", release_key.to_string()));
        }
        let kind = release.dataset_kind;
        let pointer = snapshot.pointer(kind)?;
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
        // Compose fixed statements only. Both reads use precisely the same SQL shape.
        let query = format!(
            "{FENCE} LET $observed = {{ {} }}; {CHECK_SNAPSHOT} {ACTIVATE}",
            super::active::ACTIVE_AUTHORITIES
        );
        let mut response = self
            .client()
            .query(query)
            .bind((
                "fence",
                time_record(
                    "time_authority_activation_fence",
                    identity.tenant_id.to_string(),
                ),
            ))
            .bind(("activation_token", uuid::Uuid::now_v7()))
            .bind(("kind", Option::<TimeDatasetKind>::None))
            .bind(("expected_authorities", snapshot.rows))
            .bind(("expected_candidate", release))
            .bind(("active", time_record("time_active_authority", active_key)))
            .bind((
                "previous_release",
                pointer
                    .as_ref()
                    .map(|record| time_record("time_authority_release", &record.release_key)),
            ))
            .bind((
                "release",
                time_record("time_authority_release", &release_key),
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
            .await?;
        if let Some(error) =
            veoveo_platform_store::primary_transaction_error(response.take_errors())
        {
            let message = error.to_string();
            let conflict_markers = [
                "time_authority_pair_conflict",
                "time_authority_candidate_conflict",
                "time_authority_release_conflict",
                "time_active_authority_conflict",
                "time_previous_authority_conflict",
            ];
            if matches!(
                error.query_details(),
                Some(surrealdb::types::QueryError::TransactionConflict)
            ) || conflict_markers
                .iter()
                .any(|marker| message.ends_with(marker))
            {
                return Err(conflict("authority activation", release_key.to_string()));
            }
            return Err(PersistenceError::Database(error));
        }
        self.time_authority_release(identity.tenant_id, &release_key)
            .await?
            .ok_or(PersistenceError::MissingRecord {
                operation: "time authority activation readback",
            })
    }
}

#[cfg(test)]
impl TimePersistence {
    pub(super) async fn activate_time_authority_release(
        &self,
        identity: &PlatformIdentity,
        release_key: &AuthorityReleaseId,
        expected_release: TimeVersion,
        expected_pointer: TimeWriteGuard,
        canonical_json: String,
    ) -> Result<TimeAuthorityReleaseRecord, PersistenceError> {
        let candidate = self
            .time_authority_release(identity.tenant_id, release_key)
            .await?
            .ok_or_else(|| conflict("authority release", release_key.to_string()))?;
        let snapshot = self.active_authority_snapshot(identity.tenant_id).await?;
        self.commit_time_authority_release(
            identity,
            AuthorityActivation {
                candidate,
                snapshot,
                expected_release,
                expected_pointer,
                canonical_json,
            },
        )
        .await
    }
}
