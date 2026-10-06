//! Release reads bind every URI parent and select pages before returning rows.
use super::{invalid_map, map_record};
use crate::persistence::{MapDatasetReleaseRecord, MapRepository, MapStoreError};
use veoveo_platform_store::TenantId;

impl MapRepository {
    pub async fn map_release_in_dataset(
        &self,
        tenant: TenantId,
        dataset: &crate::contract::MapDatasetId,
        release: &crate::contract::DatasetReleaseId,
    ) -> Result<Option<MapDatasetReleaseRecord>, MapStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map/releases/map_release_in_dataset.surql"
            ))
            .bind(("record", map_record("map_dataset_release", release)))
            .bind(("tenant", tenant.record_id()))
            .bind(("dataset", dataset.to_string()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Select only referenced releases; callers apply their own lifecycle policy.
    pub async fn map_release_set(
        &self,
        tenant: TenantId,
        releases: &std::collections::BTreeSet<crate::contract::DatasetReleaseId>,
    ) -> Result<Vec<MapDatasetReleaseRecord>, MapStoreError> {
        if releases.is_empty() {
            return Ok(Vec::new());
        }
        let mut response = self
            .client()
            .query(include_str!("../queries/map/releases/exact_set.surql"))
            .bind(("tenant", tenant.record_id()))
            .bind((
                "releases",
                releases.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Ascending immutable release keys, with optional dataset selection in SQL.
    /// Callers may request one lookahead row for a 100-item resource page.
    pub async fn map_releases_page(
        &self,
        tenant: TenantId,
        dataset: Option<&crate::contract::MapDatasetId>,
        after: Option<&crate::contract::DatasetReleaseId>,
        limit: usize,
    ) -> Result<Vec<MapDatasetReleaseRecord>, MapStoreError> {
        if !(1..=101).contains(&limit) {
            return Err(invalid_map("limit", "release pages require 1..=101 rows"));
        }
        let sql = match (dataset.is_some(), after.is_some()) {
            (false, false) => include_str!("../queries/map/releases/all_first.surql"),
            (false, true) => include_str!("../queries/map/releases/all_after.surql"),
            (true, false) => include_str!("../queries/map/releases/dataset_first.surql"),
            (true, true) => include_str!("../queries/map/releases/dataset_after.surql"),
        };
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", tenant.record_id()))
            .bind(("dataset", dataset.map(ToString::to_string)))
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}
