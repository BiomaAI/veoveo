//! Select routing's active release/source relationships in one database statement.
use super::{MapAccessContext, MapCatalog, decode, wire};
use crate::contract::{
    DatasetRelease, DatasetReleaseId, DatasetReleaseState, MapDatasetId, MapFamily, MapSourceId,
    RegisteredSource,
};
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::BTreeSet;

const SELECT_AUTHORITY: &str = "SELECT pointer_id, pointer_version, dataset_key,
    release.release_key AS release_key, release.source_key AS source_key,
    release.state AS release_state, release.valid_from AS valid_from,
    release.valid_until AS valid_until, release.record_version AS release_version,
    source.record_version AS source_version, source.map_families AS source_families,
    array::intersect(source.map_families, $families) AS families,
    release.canonical_json AS release_json, source.canonical_json AS source_json
FROM (
    SELECT pointer_id, pointer_version, dataset_key, release, source FROM (
        SELECT pointer_id, pointer_version, dataset_key, release,
            type::record('map_source', release.source_key) AS source FROM (
            SELECT record::id(id) AS pointer_id, record_version AS pointer_version,
                dataset_key, release_key,
                type::record('map_dataset_release', release_key) AS release
            FROM map_active_release WHERE tenant = $tenant
        ) WHERE release.tenant = $tenant AND release.release_key = release_key
            AND release.dataset_key = dataset_key AND release.state = 'active'
            AND release.valid_from <= $at
            AND (release.valid_until = NONE OR release.valid_until > $at)
    ) WHERE source.tenant = $tenant AND source.enabled = true
        AND source.source_key = release.source_key AND source.dataset_key = dataset_key
) WHERE source.map_families CONTAINSANY $families
ORDER BY release_key ASC TIMEOUT 5s;";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingAuthority {
    release_ids: BTreeSet<DatasetReleaseId>,
    map_families: BTreeSet<MapFamily>,
}
impl RoutingAuthority {
    pub fn into_parts(self) -> (BTreeSet<DatasetReleaseId>, BTreeSet<MapFamily>) {
        (self.release_ids, self.map_families)
    }
}

impl MapCatalog {
    /// Complete internal planning input; this is not a public catalog page.
    /// ```compile_fail
    /// use veoveo_map_mcp::catalog::{MapCatalog, MapAccessContext};
    /// use std::collections::BTreeSet;
    /// async fn wrong(catalog: MapCatalog, scope: MapAccessContext) {
    ///     catalog.routing_authority(&scope, &BTreeSet::from(["road_street"]), chrono::Utc::now()).await;
    /// }
    /// ```
    pub async fn routing_authority(
        &self,
        scope: &MapAccessContext,
        families: &BTreeSet<MapFamily>,
        at: DateTime<Utc>,
    ) -> Result<RoutingAuthority> {
        let mut response = self
            .store()
            .client()
            .query(SELECT_AUTHORITY)
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("at", at))
            .bind((
                "families",
                families.iter().map(wire).collect::<Result<Vec<_>>>()?,
            ))
            .await?
            .check()?;
        let rows: Vec<serde_json::Value> = response.take(0)?;
        let mut authority = RoutingAuthority {
            release_ids: BTreeSet::new(),
            map_families: BTreeSet::new(),
        };
        for row in rows {
            let row: AuthorityRow = serde_json::from_value(row)?;
            row.validate(scope, families)?;
            ensure!(
                authority.release_ids.insert(row.release_key),
                "duplicate routing release identity"
            );
            authority.map_families.extend(row.families);
        }
        Ok(authority)
    }
}

#[derive(Deserialize)]
struct AuthorityRow {
    pointer_id: String,
    pointer_version: u64,
    dataset_key: MapDatasetId,
    source_key: MapSourceId,
    release_key: DatasetReleaseId,
    release_state: DatasetReleaseState,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    release_version: u64,
    source_version: u64,
    source_families: BTreeSet<MapFamily>,
    families: BTreeSet<MapFamily>,
    release_json: String,
    source_json: String,
}
impl AuthorityRow {
    fn validate(&self, scope: &MapAccessContext, families: &BTreeSet<MapFamily>) -> Result<()> {
        let release: DatasetRelease = decode(&self.release_json, "routing release")?;
        let source: RegisteredSource = decode(&self.source_json, "routing source")?;
        release.validate()?;
        source.validate()?;
        ensure!(
            self.pointer_version > 0
                && self.pointer_id == format!("{}:{}", scope.identity.tenant_id, self.dataset_key)
                && release.release_id == self.release_key
                && release.dataset_id == self.dataset_key
                && release.source_id == self.source_key
                && release.state == self.release_state
                && release.valid_from == self.valid_from
                && release.valid_until == self.valid_until
                && release.record_version == self.release_version
                && source.source_id == self.source_key
                && source.dataset_id == self.dataset_key
                && source.enabled
                && source.map_families == self.source_families
                && source.record_version == self.source_version
                && !self.families.is_empty()
                && self.families
                    == self
                        .source_families
                        .intersection(families)
                        .copied()
                        .collect(),
            "routing authority pointer, indexed fields or retained documents disagree"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
