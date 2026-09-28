//! Bounded catalog completion with matching and deduplication inside SQL.
use crate::{PlatformIdentity, PlatformStore, StoreError};

#[derive(Clone, Debug)]
pub enum MapCatalogCompletion {
    Source,
    Dataset,
    Release { dataset: Option<String> },
    MobilityProfile,
    MobilityProfileVersion { profile: Option<String> },
    Route,
    Matrix,
}

impl PlatformStore {
    pub async fn complete_map_catalog(
        &self,
        identity: &PlatformIdentity,
        domain: MapCatalogCompletion,
        needle: &str,
    ) -> Result<Vec<String>, StoreError> {
        validate_needle(needle)?;
        let (table, field, predicate, parent) = match domain {
            MapCatalogCompletion::Source => ("map_source", "source_key", "true", None),
            MapCatalogCompletion::Dataset => ("map_dataset_release", "dataset_key", "true", None),
            MapCatalogCompletion::Release { dataset } => (
                "map_dataset_release",
                "release_key",
                "($parent = NONE OR dataset_key = $parent)",
                dataset,
            ),
            MapCatalogCompletion::MobilityProfile => {
                ("map_mobility_profile", "profile_key", "true", None)
            }
            MapCatalogCompletion::MobilityProfileVersion { profile } => (
                "map_mobility_profile",
                "type::string(profile_version)",
                "($parent = NONE OR profile_key = $parent)",
                profile,
            ),
            MapCatalogCompletion::Route => ("map_route", "route_key", "owner = $owner", None),
            MapCatalogCompletion::Matrix => (
                "map_route_matrix",
                "matrix_key",
                "owner = $owner AND canonical_json != NONE",
                None,
            ),
        };
        // Identifiers and expressions come only from the closed domain enum.
        let sql = format!(
            "SELECT VALUE candidate FROM (SELECT {field} AS candidate FROM {table} WHERE tenant = $tenant AND {predicate} AND string::lowercase({field}) CONTAINS $needle GROUP BY candidate ORDER BY candidate ASC LIMIT 101);"
        );
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("parent", parent))
            .bind(("needle", needle.to_lowercase()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}

pub(crate) fn validate_needle(needle: &str) -> Result<(), StoreError> {
    if needle.len() > 512 || needle.chars().any(char::is_control) {
        return Err(super::invalid_map(
            "completion",
            "must be at most 512 bytes without control characters",
        ));
    }
    Ok(())
}
