//! Owner-scoped Map reads and SQL selection for catalog maintenance.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};

use super::{MAX_ROUTE_JSON_BYTES, RouteLookups, invalid_map, map_record, validate_json};
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
        key: &crate::contract::RouteId,
    ) -> Result<Option<MapRouteRecord>, MapStoreError> {
        let row: Option<MapRouteRecord> =
            select_owned(self, identity, map_record("map_route", key)).await?;
        if let Some(row) = &row {
            if row.id != map_record("map_route", key) {
                return Err(invalid_map("route", "physical identity mismatch"));
            }
            RouteLookups::from_document(key, &row.canonical_json)?.verify_record(row)?;
        }
        Ok(row)
    }

    pub async fn map_route_matrix(
        &self,
        identity: &PlatformIdentity,
        key: &crate::contract::RouteMatrixId,
    ) -> Result<Option<MapRouteMatrixRecord>, MapStoreError> {
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
        key: &crate::contract::AcquisitionId,
    ) -> Result<Option<MapAcquisitionRecord>, MapStoreError> {
        select_owned(self, identity, map_record("map_acquisition", key)).await
    }

    pub async fn map_routes_page(
        &self,
        identity: &PlatformIdentity,
        after: Option<&crate::contract::RouteId>,
        limit: usize,
    ) -> Result<Vec<MapRouteIndexRecord>, MapStoreError> {
        validate_page(limit)?;
        let mut response = self
            .client()
            .query(include_str!("../queries/map/owned/map_routes_page.surql"))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn map_matrices_page(
        &self,
        identity: &PlatformIdentity,
        after: Option<&crate::contract::RouteMatrixId>,
        limit: usize,
    ) -> Result<Vec<MapMatrixIndexRecord>, MapStoreError> {
        validate_page(limit)?;
        let mut response = self
            .client()
            .query(include_str!("../queries/map/owned/map_matrices_page.surql"))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn map_acquisitions_page(
        &self,
        identity: &PlatformIdentity,
        after: Option<&crate::contract::AcquisitionId>,
        limit: usize,
    ) -> Result<Vec<MapAcquisitionRecord>, MapStoreError> {
        validate_page(limit)?;
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map/owned/map_acquisitions_page.surql"
            ))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Single-writer acquisition recovery supplies the synchronized worker inventory.
    pub async fn map_interrupted_acquisitions_page(
        &self,
        identity: &PlatformIdentity,
        active: &[crate::contract::AcquisitionId],
        after: Option<&crate::contract::AcquisitionId>,
        limit: usize,
    ) -> Result<Vec<MapAcquisitionRecord>, MapStoreError> {
        validate_page(limit)?;
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map/owned/map_interrupted_acquisitions_page.surql"
            ))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("owner", identity.principal_id.record_id()))
            .bind((
                "active",
                active.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ))
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Dataset/restriction administration invalidates affected routes across owners.
    pub async fn map_routes_for_dependency_page(
        &self,
        tenant: TenantId,
        dependency: &super::MapDependencyIdentity,
        after: Option<&crate::contract::RouteId>,
        limit: usize,
    ) -> Result<Vec<MapRouteRecord>, MapStoreError> {
        validate_page(limit)?;

        // The complete route is the committed source for dependencies. Its
        // separately written dependency rows can be incomplete after interruption.
        let sql = match dependency.kind() {
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
            .bind(("dependency", dependency.key().to_owned()))
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", limit))
            .await?
            .check()?;
        let rows: Vec<MapRouteRecord> = response.take(0)?;
        for row in &rows {
            if row.id != map_record("map_route", &row.route_key) {
                return Err(invalid_map("route", "physical identity mismatch"));
            }
            let route_id = crate::contract::RouteId::parse(&row.route_key)
                .map_err(|_| invalid_map("route_key", "invalid retained route identity"))?;
            RouteLookups::from_document(&route_id, &row.canonical_json)?.verify_record(row)?;
        }
        Ok(rows)
    }

    /// The domain supplies the same invalidated state in its complete route document.
    pub async fn invalidate_map_route(
        &self,
        tenant: TenantId,
        key: &crate::contract::RouteId,
        canonical_json: String,
    ) -> Result<bool, MapStoreError> {
        validate_json("canonical_json", &canonical_json, MAX_ROUTE_JSON_BYTES)?;
        let route = RouteLookups::from_document(key, &canonical_json)?;
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map/owned/invalidate_map_route.surql"
            ))
            .bind(("record", map_record("map_route", key)))
            .bind(("tenant", tenant.record_id()))
            .bind(("canonical_json", canonical_json))
            .bind((
                "base_release_ids",
                route
                    .base_release_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "restriction_ids",
                route
                    .restriction_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "facility_ids",
                route
                    .facility_ids
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>(),
            ))
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

fn validate_page(limit: usize) -> Result<(), MapStoreError> {
    if !(1..=101).contains(&limit) {
        return Err(invalid_map("limit", "Map pages require 1..=101 rows"));
    }
    Ok(())
}
