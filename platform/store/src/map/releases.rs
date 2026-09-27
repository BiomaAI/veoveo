//! Release reads bind every URI parent and select pages before returning rows.
use super::{invalid_map, map_record, validate_public_key};
use crate::{MapDatasetReleaseRecord, PlatformStore, StoreError, TenantId};

impl PlatformStore {
    pub async fn map_release_in_dataset(
        &self,
        tenant: TenantId,
        dataset: &str,
        release: &str,
    ) -> Result<Option<MapDatasetReleaseRecord>, StoreError> {
        validate_public_key("dataset_key", dataset, "dataset-")?;
        validate_public_key("release_key", release, "release-")?;
        let mut response = self
            .client()
            .query("SELECT * FROM ONLY $record WHERE tenant = $tenant AND dataset_key = $dataset;")
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
    ) -> Result<Vec<MapDatasetReleaseRecord>, StoreError> {
        if !(1..=101).contains(&limit) {
            return Err(invalid_map("limit", "release pages require 1..=101 rows"));
        }
        if let Some(dataset) = dataset {
            validate_public_key("dataset_key", dataset, "dataset-")?;
        }
        if let Some(after) = after {
            validate_public_key("after", after, "release-")?;
        }
        let parent = if dataset.is_some() {
            " AND dataset_key = $dataset"
        } else {
            ""
        };
        let seek = if after.is_some() {
            " AND release_key > $after"
        } else {
            ""
        };
        let mut response = self.client().query(format!(
            "SELECT * FROM map_dataset_release WHERE tenant = $tenant{parent}{seek} ORDER BY release_key ASC LIMIT $limit;"
        ))
        .bind(("tenant", tenant.record_id()))
        .bind(("dataset", dataset.map(ToOwned::to_owned)))
        .bind(("after", after.map(ToOwned::to_owned)))
        .bind(("limit", limit))
        .await?.check()?;
        Ok(response.take(0)?)
    }
}
