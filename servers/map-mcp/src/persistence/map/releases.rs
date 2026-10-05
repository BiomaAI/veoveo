//! Release reads bind every URI parent and select pages before returning rows.
use super::{invalid_map, map_record, validate_public_key};
use crate::persistence::{MapDatasetReleaseRecord, MapRepository, MapStoreError};
use veoveo_platform_store::TenantId;

impl MapRepository {
    pub async fn map_release_in_dataset(
        &self,
        tenant: TenantId,
        dataset: &str,
        release: &str,
    ) -> Result<Option<MapDatasetReleaseRecord>, MapStoreError> {
        validate_public_key("dataset_key", dataset, "dataset-")?;
        validate_public_key("release_key", release, "release-")?;
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map/releases/map_release_in_dataset.surql"
            ))
            .bind(("record", map_record("map_dataset_release", release)))
            .bind(("tenant", tenant.record_id()))
            .bind(("dataset", dataset.to_owned()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Ascending immutable release keys, with optional dataset selection in SQL.
    /// Callers may request one lookahead row for a 100-item resource page.
    pub async fn map_releases_page(
        &self,
        tenant: TenantId,
        dataset: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapDatasetReleaseRecord>, MapStoreError> {
        if !(1..=101).contains(&limit) {
            return Err(invalid_map("limit", "release pages require 1..=101 rows"));
        }
        if let Some(dataset) = dataset {
            validate_public_key("dataset_key", dataset, "dataset-")?;
        }
        if let Some(after) = after {
            validate_public_key("after", after, "release-")?;
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
            .bind(("dataset", dataset.map(ToOwned::to_owned)))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}
