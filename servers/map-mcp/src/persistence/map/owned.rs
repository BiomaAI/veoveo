//! Owner-scoped Map reads and SQL selection for catalog maintenance.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};

use super::{
    MAX_ROUTE_JSON_BYTES, invalid_map, map_record, validate_json, validate_public_key,
    validate_text,
};
use crate::persistence::{
    MapAcquisitionRecord, MapDependencyKind, MapRepository, MapRouteMatrixRecord, MapRouteRecord,
    MapRouteState, MapStoreError,
};
use veoveo_platform_store::{PlatformIdentity, TenantId};

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub struct MapRouteIndexRecord {
    pub route_key: String,
    pub status: MapRouteState,
    pub mobility_profile_key: String,
    pub mobility_profile_version: i64,
    pub departure_time: DateTime<Utc>,
    pub arrival_time: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub struct MapMatrixIndexRecord {
    pub matrix_key: String,
    pub mobility_profile_key: String,
    pub mobility_profile_version: i64,
    pub created_at: DateTime<Utc>,
}

impl MapRepository {
    pub async fn map_route(
        &self,
        identity: &PlatformIdentity,
        key: &str,
    ) -> Result<Option<MapRouteRecord>, MapStoreError> {
        validate_public_key("route_key", key, "route-")?;
        select_owned(self, identity, map_record("map_route", key)).await
    }

    pub async fn map_route_matrix(
        &self,
        identity: &PlatformIdentity,
        key: &str,
    ) -> Result<Option<MapRouteMatrixRecord>, MapStoreError> {
        validate_public_key("matrix_key", key, "matrix-")?;
        let mut response = self
            .client()
            .query(include_str!("../queries/map/owned/map_route_matrix.surql"))
            .bind(("record", map_record("map_route_matrix", key)))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn map_acquisition(
        &self,
        identity: &PlatformIdentity,
        key: &str,
    ) -> Result<Option<MapAcquisitionRecord>, MapStoreError> {
        validate_public_key("acquisition_key", key, "acquisition-")?;
        select_owned(self, identity, map_record("map_acquisition", key)).await
    }

    pub async fn map_routes_page(
        &self,
        identity: &PlatformIdentity,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapRouteIndexRecord>, MapStoreError> {
        validate_page(after, "route-", limit)?;
        let mut response = self
            .client()
            .query(include_str!("../queries/map/owned/map_routes_page.surql"))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn map_matrices_page(
        &self,
        identity: &PlatformIdentity,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapMatrixIndexRecord>, MapStoreError> {
        validate_page(after, "matrix-", limit)?;
        let mut response = self
            .client()
            .query(include_str!("../queries/map/owned/map_matrices_page.surql"))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn map_acquisitions_page(
        &self,
        identity: &PlatformIdentity,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapAcquisitionRecord>, MapStoreError> {
        validate_page(after, "acquisition-", limit)?;
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map/owned/map_acquisitions_page.surql"
            ))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Single-writer acquisition recovery supplies the synchronized worker inventory.
    pub async fn map_interrupted_acquisitions_page(
        &self,
        identity: &PlatformIdentity,
        active: &[String],
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapAcquisitionRecord>, MapStoreError> {
        validate_page(after, "acquisition-", limit)?;
        for key in active {
            validate_public_key("active", key, "acquisition-")?;
        }
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map/owned/map_interrupted_acquisitions_page.surql"
            ))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("active", active.to_vec()))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Dataset/restriction administration invalidates affected routes across owners.
    pub async fn map_routes_for_dependency_page(
        &self,
        tenant: TenantId,
        kind: MapDependencyKind,
        dependency: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapRouteRecord>, MapStoreError> {
        validate_page(after, "route-", limit)?;
        validate_text("dependency", dependency, 256)?;
        // The complete route is the committed source for dependencies. Its
        // separately written dependency rows can be incomplete after interruption.
        let sql = match kind {
            MapDependencyKind::Release => {
                include_str!("../queries/map/owned/dependency_release.surql")
            }
            MapDependencyKind::Restriction => {
                include_str!("../queries/map/owned/dependency_restriction.surql")
            }
            MapDependencyKind::Facility => {
                include_str!("../queries/map/owned/dependency_facility.surql")
            }
        };
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", tenant.record_id()))
            .bind(("dependency", dependency.to_owned()))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// The domain supplies the same invalidated state in its complete route document.
    pub async fn invalidate_map_route(
        &self,
        tenant: TenantId,
        key: &str,
        canonical_json: String,
    ) -> Result<bool, MapStoreError> {
        validate_public_key("route_key", key, "route-")?;
        validate_json("canonical_json", &canonical_json, MAX_ROUTE_JSON_BYTES)?;
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map/owned/invalidate_map_route.surql"
            ))
            .bind(("record", map_record("map_route", key)))
            .bind(("tenant", tenant.record_id()))
            .bind(("canonical_json", canonical_json))
            .await?
            .check()?;
        Ok(response.take::<Option<MapRouteRecord>>(0)?.is_some())
    }
}

pub(super) async fn select_owned<T: for<'de> Deserialize<'de> + SurrealValue>(
    store: &MapRepository,
    identity: &PlatformIdentity,
    record: RecordId,
) -> Result<Option<T>, MapStoreError> {
    let mut response = store
        .client()
        .query(include_str!("../queries/map/owned/select_owned.surql"))
        .bind(("record", record))
        .bind(("tenant", identity.tenant_id.record_id()))
        .bind(("owner", identity.principal_id.record_id()))
        .await?
        .check()?;
    Ok(response.take(0)?)
}

fn validate_page(
    after: Option<&str>,
    prefix: &'static str,
    limit: usize,
) -> Result<(), MapStoreError> {
    if !(1..=101).contains(&limit) {
        return Err(invalid_map("limit", "Map pages require 1..=101 rows"));
    }
    if let Some(after) = after {
        validate_public_key("after", after, prefix)?;
    }
    Ok(())
}
