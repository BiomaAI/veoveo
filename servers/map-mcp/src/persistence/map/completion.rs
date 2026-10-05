//! Bounded catalog completion with matching and deduplication inside SQL.
use crate::persistence::{MapRepository, MapStoreError};
use veoveo_platform_store::PlatformIdentity;

#[derive(Clone, Debug)]
pub enum MapCatalogCompletion {
    Dataset,
    Release { dataset: Option<String> },
    Route,
    Matrix,
}

impl MapRepository {
    pub async fn complete_map_catalog(
        &self,
        identity: &PlatformIdentity,
        domain: MapCatalogCompletion,
        needle: &str,
    ) -> Result<Vec<String>, MapStoreError> {
        validate_needle(needle)?;
        let (sql, parent) = match domain {
            MapCatalogCompletion::Dataset => (
                include_str!("../queries/map/completion/dataset.surql"),
                None,
            ),
            MapCatalogCompletion::Release { dataset } => (
                include_str!("../queries/map/completion/release.surql"),
                dataset,
            ),
            MapCatalogCompletion::Route => {
                (include_str!("../queries/map/completion/route.surql"), None)
            }
            MapCatalogCompletion::Matrix => {
                (include_str!("../queries/map/completion/matrix.surql"), None)
            }
        };
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

pub(crate) fn validate_needle(needle: &str) -> Result<(), MapStoreError> {
    if needle.len() > 512 || needle.chars().any(char::is_control) {
        return Err(super::invalid_map(
            "completion",
            "must be at most 512 bytes without control characters",
        ));
    }
    Ok(())
}
