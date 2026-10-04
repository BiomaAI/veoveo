//! A tenant's active pointers and their releases are read in one SQL statement.
use super::*;

#[cfg(test)]
#[path = "active_tests.rs"]
mod tests;

pub(super) const ACTIVE_AUTHORITIES: &str = r#"
SELECT dataset_kind, $this AS pointer,
    (SELECT * FROM type::record('time_authority_release', $parent.release_key)
        WHERE tenant = $tenant AND dataset_kind = $parent.dataset_kind
        AND release_key = $parent.release_key AND state = 'active')[0] AS release
FROM time_active_authority
WHERE tenant = $tenant AND ($kind = NONE OR dataset_kind = $kind)
ORDER BY dataset_kind ASC;
"#;

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub(super) struct ActiveAuthorityRow {
    dataset_kind: TimeDatasetKind,
    pointer: TimeActiveAuthorityRecord,
    release: Option<TimeAuthorityReleaseRecord>,
}

/// The complete, admitted SQL result observed before loading authority files.
pub(crate) struct ActiveAuthoritySnapshot {
    pub(super) tenant: TenantId,
    pub(super) rows: Vec<ActiveAuthorityRow>,
}

impl ActiveAuthoritySnapshot {
    pub(crate) fn releases(&self) -> impl Iterator<Item = TimeAuthorityReleaseRecord> + '_ {
        self.rows
            .iter()
            .map(|row| row.release.clone().expect("admitted release"))
    }

    pub(super) fn pointer(
        &self,
        kind: TimeDatasetKind,
    ) -> Result<Option<ActiveAuthority>, PersistenceError> {
        self.rows
            .iter()
            .find(|row| row.dataset_kind == kind)
            .cloned()
            .map(|row| admit(self.tenant, row))
            .transpose()
    }
}

/// Admitted pointer metadata; the catalog separately admits the release's JSON body.
#[derive(Debug)]
pub(crate) struct ActiveAuthority {
    pub(crate) release_key: AuthorityReleaseId,
    pub(crate) previous_release_key: Option<AuthorityReleaseId>,
    pub(crate) record_version: TimeVersion,
    pub(crate) release: TimeAuthorityReleaseRecord,
}

impl TimePersistence {
    pub(crate) async fn active_authority_snapshot(
        &self,
        tenant: TenantId,
    ) -> Result<ActiveAuthoritySnapshot, PersistenceError> {
        let rows = self.active_rows(tenant, None).await?;
        for row in &rows {
            admit(tenant, row.clone())?;
        }
        Ok(ActiveAuthoritySnapshot { tenant, rows })
    }

    #[cfg(test)]
    pub(crate) async fn active_time_authority(
        &self,
        tenant: TenantId,
        kind: TimeDatasetKind,
    ) -> Result<Option<ActiveAuthority>, PersistenceError> {
        Ok(self.active_authorities(tenant, Some(kind)).await?.pop())
    }

    pub(crate) async fn list_active_time_authorities(
        &self,
        tenant: TenantId,
    ) -> Result<Vec<ActiveAuthority>, PersistenceError> {
        self.active_authorities(tenant, None).await
    }

    async fn active_authorities(
        &self,
        tenant: TenantId,
        kind: Option<TimeDatasetKind>,
    ) -> Result<Vec<ActiveAuthority>, PersistenceError> {
        self.active_rows(tenant, kind)
            .await?
            .into_iter()
            .map(|row| admit(tenant, row))
            .collect()
    }

    async fn active_rows(
        &self,
        tenant: TenantId,
        kind: Option<TimeDatasetKind>,
    ) -> Result<Vec<ActiveAuthorityRow>, PersistenceError> {
        let mut response = self
            .client()
            .query(ACTIVE_AUTHORITIES)
            .bind(("tenant", tenant.record_id()))
            .bind(("kind", kind))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}

fn release_id(value: String, field: &'static str) -> Result<AuthorityReleaseId, PersistenceError> {
    validate_key(field, &value, "time-release-")?;
    AuthorityReleaseId::parse(value).map_err(|_| invalid(field, "invalid stored release identity"))
}

fn admit(tenant: TenantId, row: ActiveAuthorityRow) -> Result<ActiveAuthority, PersistenceError> {
    let pointer = row.pointer;
    let key = format!("{tenant}:{}", dataset_kind_key(pointer.dataset_kind));
    if pointer.id != time_record("time_active_authority", key)
        || pointer.tenant != tenant.record_id()
        || pointer.dataset_kind != row.dataset_kind
    {
        return Err(invalid(
            "active_authority.identity",
            "pointer key and tenant/family must agree",
        ));
    }
    let record_version = u64::try_from(pointer.record_version)
        .ok()
        .and_then(|value| TimeVersion::new(value).ok())
        .ok_or_else(|| {
            invalid(
                "active_authority.record_version",
                "must be a positive stored version",
            )
        })?;
    let release_key = release_id(pointer.release_key, "active_authority.release_key")?;
    let previous_release_key = pointer
        .previous_release_key
        .map(|value| release_id(value, "active_authority.previous_release_key"))
        .transpose()?;
    if previous_release_key.as_ref() == Some(&release_key)
        || (record_version == TimeVersion::FIRST) != previous_release_key.is_none()
    {
        return Err(invalid(
            "active_authority.previous_release_key",
            "must agree with pointer history",
        ));
    }
    // SQL excludes a release from another tenant or family, a wrong key, and a
    // non-active state. Its absence is corruption of a visible pointer, not an
    // absent pointer eligible for bootstrap authority.
    let release = row.release.ok_or_else(|| {
        invalid(
            "active_authority.release",
            "must reference an active release of the same tenant and family",
        )
    })?;
    validate_positive(
        "active_authority.release.record_version",
        release.record_version,
    )?;
    Ok(ActiveAuthority {
        release_key,
        previous_release_key,
        record_version,
        release,
    })
}
