//! Optimistic authority publication rechecks relationships in its transaction.
use super::*;

/// Only the catalog constructs a write from a checked candidate and active snapshot.
pub(crate) struct AuthorityActivation {
    pub(crate) candidate: TimeAuthorityReleaseRecord,
    pub(crate) snapshot: ActiveAuthoritySnapshot,
    pub(crate) expected_release: TimeVersion,
    pub(crate) expected_pointer: TimeWriteGuard,
    pub(crate) canonical_json: String,
}

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
        let release_key = AuthorityReleaseId::parse(&release.release_key)
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
        let mut response = self
            .client()
            .query(include_str!("queries/activate_authority.surql"))
            .bind((
                "pointers",
                [TimeDatasetKind::LeapSeconds, TimeDatasetKind::Tzdb].map(|kind| {
                    time_record(
                        "time_active_authority",
                        format!("{}:{}", identity.tenant_id, dataset_kind_key(kind)),
                    )
                }),
            ))
            .bind((
                "releases",
                snapshot
                    .releases()
                    .map(|record| record.release_key)
                    .chain(std::iter::once(release_key.to_string()))
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .map(|key| time_record("time_authority_release", key))
                    .collect::<Vec<_>>(),
            ))
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
