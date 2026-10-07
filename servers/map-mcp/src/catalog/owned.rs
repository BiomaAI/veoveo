//! Owner-scoped catalog pages and database-selected maintenance batches.
use crate::persistence::MapRepository;
use crate::persistence::{MapDependencyIdentity, MapRouteState};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{MapAccessContext, MapCatalog, decode, encode};
use crate::contract::{
    AcquisitionId, AcquisitionJob, AcquisitionStatus, DatasetReleaseId, MapCatalogPage,
    MapMatrixUri, MapRouteUri, MatrixSummary, MatrixSummaryBuilder, RestrictionId, RouteId,
    RouteMatrix, RouteMatrixId, RoutePlan, RouteStatus, RouteSummary, RouteSummaryBuilder,
};

pub const PAGE_SIZE: usize = 100;

pub use crate::contract::OwnedPage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Collection {
    Routes,
    Matrices,
    Acquisitions,
}

impl Collection {
    pub fn selection(self) -> MapCatalogPage {
        match self {
            Self::Routes => MapCatalogPage::Routes { after: None },
            Self::Matrices => MapCatalogPage::Matrices { after: None },
            Self::Acquisitions => MapCatalogPage::Acquisitions { after: None },
        }
    }

    fn page<T, U>(
        self,
        mut rows: Vec<T>,
        key: impl Fn(&T) -> &str,
        convert: impl Fn(T) -> Result<U>,
    ) -> Result<OwnedPage<U>> {
        let more = rows.len() > PAGE_SIZE;
        rows.truncate(PAGE_SIZE);
        let next_cursor = if more {
            let key = key(rows.last().expect("nonempty page"));
            match self {
                Self::Routes => MapCatalogPage::Routes {
                    after: Some(key.parse()?),
                },
                Self::Matrices => MapCatalogPage::Matrices {
                    after: Some(key.parse()?),
                },
                Self::Acquisitions => MapCatalogPage::Acquisitions {
                    after: Some(key.parse()?),
                },
            }
            .cursor()
        } else {
            None
        };
        Ok(OwnedPage {
            items: rows.into_iter().map(convert).collect::<Result<_>>()?,
            limit: PAGE_SIZE,
            next_cursor,
        })
    }
}

impl MapCatalog {
    pub async fn route(&self, scope: &MapAccessContext, id: &RouteId) -> Result<Option<RoutePlan>> {
        MapRepository::new(self.store().clone())
            .map_route(&scope.identity, id)
            .await?
            .map(|row| {
                let route: RoutePlan = decode(&row.canonical_json, "route")?;
                anyhow::ensure!(
                    route.route_id == *id && route.route_uri.id() == id,
                    "route document disagrees with its selected identity"
                );
                Ok(route)
            })
            .transpose()
    }

    pub async fn matrix(
        &self,
        scope: &MapAccessContext,
        id: &RouteMatrixId,
    ) -> Result<Option<RouteMatrix>> {
        MapRepository::new(self.store().clone())
            .map_route_matrix(&scope.identity, id)
            .await?
            .map(|row| {
                let value: RouteMatrix = decode(
                    row.canonical_json
                        .as_deref()
                        .context("matrix document missing")?,
                    "route matrix",
                )?;
                anyhow::ensure!(
                    row.id
                        == surrealdb::types::RecordId::new(
                            "map_route_matrix",
                            row.matrix_key.clone()
                        )
                        && value.matrix_id == *id
                        && value.matrix_id.as_str() == row.matrix_key
                        && value.provenance.operational_snapshot_id.as_str()
                            == row.operational_snapshot_key,
                    "matrix document disagrees with selected identity or metadata"
                );
                Ok(value)
            })
            .transpose()
    }

    pub async fn acquisition(
        &self,
        scope: &MapAccessContext,
        id: &AcquisitionId,
    ) -> Result<Option<AcquisitionJob>> {
        MapRepository::new(self.store().clone())
            .map_acquisition(&scope.identity, id)
            .await?
            .map(checked_acquisition)
            .transpose()
    }

    pub async fn routes_page(
        &self,
        scope: &MapAccessContext,
        after: Option<&RouteId>,
    ) -> Result<OwnedPage<RouteSummary>> {
        let rows = MapRepository::new(self.store().clone())
            .map_routes_page(&scope.identity, after, PAGE_SIZE + 1)
            .await?;
        Collection::Routes.page(
            rows,
            |row| &row.route_key,
            |row| {
                let route_id: RouteId = row.route_key.parse()?;
                Ok(RouteSummaryBuilder {
                    resource_uri: MapRouteUri::new(route_id.clone()),
                    route_id,
                    status: match row.status {
                        MapRouteState::PlanningAdvisory => RouteStatus::PlanningAdvisory,
                        MapRouteState::Validated => RouteStatus::Validated,
                        MapRouteState::Stale => RouteStatus::Stale,
                        MapRouteState::Invalidated => RouteStatus::Invalidated,
                        MapRouteState::Unavailable => RouteStatus::Unavailable,
                    },
                    mobility_profile_id: row.mobility_profile_key.parse()?,
                    mobility_profile_version: row.mobility_profile_version.try_into()?,
                    departure_time: row.departure_time,
                    arrival_time: row.arrival_time,
                    created_at: row.created_at,
                }
                .build()?)
            },
        )
    }

    pub async fn matrices_page(
        &self,
        scope: &MapAccessContext,
        after: Option<&RouteMatrixId>,
    ) -> Result<OwnedPage<MatrixSummary>> {
        let rows = MapRepository::new(self.store().clone())
            .map_matrices_page(&scope.identity, after, PAGE_SIZE + 1)
            .await?;
        Collection::Matrices.page(
            rows,
            |row| &row.matrix_key,
            |row| {
                let matrix_id: RouteMatrixId = row.matrix_key.parse()?;
                Ok(MatrixSummaryBuilder {
                    resource_uri: MapMatrixUri::new(matrix_id.clone()),
                    matrix_id,
                    mobility_profile_id: row.mobility_profile_key.parse()?,
                    mobility_profile_version: row.mobility_profile_version.try_into()?,
                    created_at: row.created_at,
                }
                .build()?)
            },
        )
    }

    pub async fn acquisitions_page(
        &self,
        scope: &MapAccessContext,
        after: Option<&AcquisitionId>,
    ) -> Result<OwnedPage<AcquisitionJob>> {
        let rows = MapRepository::new(self.store().clone())
            .map_acquisitions_page(&scope.identity, after, PAGE_SIZE + 1)
            .await?;
        Collection::Acquisitions.page(rows, |row| &row.acquisition_key, checked_acquisition)
    }

    async fn interrupted_acquisitions_batch(
        &self,
        scope: &MapAccessContext,
        active: &[AcquisitionId],
        after: Option<&AcquisitionId>,
    ) -> Result<Vec<AcquisitionJob>> {
        MapRepository::new(self.store().clone())
            .map_interrupted_acquisitions_page(&scope.identity, active, after, PAGE_SIZE)
            .await?
            .into_iter()
            .map(checked_acquisition)
            .collect()
    }

    pub async fn reconcile_interrupted_acquisitions(
        &self,
        scope: &MapAccessContext,
        active: &[AcquisitionId],
    ) -> Result<()> {
        let mut after = None;
        loop {
            let jobs = self
                .interrupted_acquisitions_batch(scope, active, after.as_ref())
                .await?;
            if jobs.is_empty() {
                break;
            }
            after = jobs.last().map(|job| job.acquisition_id.clone());
            for mut job in jobs {
                job.status = AcquisitionStatus::Failed;
                job.progress.message =
                    "acquisition was interrupted by a Map server restart".to_owned();
                self.update_acquisition(scope, job).await?;
            }
        }
        Ok(())
    }

    pub async fn invalidate_routes_for_release(
        &self,
        scope: &MapAccessContext,
        id: &DatasetReleaseId,
    ) -> Result<u64> {
        self.invalidate_routes(scope, MapDependencyIdentity::Release(id.clone()))
            .await
    }

    pub async fn invalidate_routes_for_restriction(
        &self,
        scope: &MapAccessContext,
        id: &RestrictionId,
    ) -> Result<u64> {
        self.invalidate_routes(scope, MapDependencyIdentity::Restriction(id.clone()))
            .await
    }

    async fn invalidate_routes(
        &self,
        scope: &MapAccessContext,
        dependency: MapDependencyIdentity,
    ) -> Result<u64> {
        let mut after = None;
        let mut count = 0;
        loop {
            let rows = MapRepository::new(self.store().clone())
                .map_routes_for_dependency_page(
                    scope.identity.tenant_id,
                    &dependency,
                    after.as_ref(),
                    PAGE_SIZE,
                )
                .await?;
            if rows.is_empty() {
                break;
            }
            after = rows
                .last()
                .map(|row| row.route_key.parse::<RouteId>())
                .transpose()?;
            for row in rows {
                let route: RoutePlan = decode(&row.canonical_json, "route")?;
                let mut route = route.into_value();
                route.status = RouteStatus::Invalidated;
                let route = RoutePlan::new(route)?;
                count += u64::from(
                    MapRepository::new(self.store().clone())
                        .invalidate_map_route(
                            scope.identity.tenant_id,
                            &route.route_id,
                            encode(&route)?,
                        )
                        .await?,
                );
            }
        }
        Ok(count)
    }
}

fn checked_acquisition(row: crate::persistence::MapAcquisitionRecord) -> Result<AcquisitionJob> {
    let value: AcquisitionJob = decode(&row.canonical_json, "acquisition job")?;
    anyhow::ensure!(
        row.id == surrealdb::types::RecordId::new("map_acquisition", row.acquisition_key.clone())
            && value.acquisition_id.as_str() == row.acquisition_key
            && value.source_id.as_str() == row.source_key
            && super::acquisition_state_to_store(value.status) == row.status
            && super::wire(&value.progress.phase)? == row.phase
            && value.staged_release_id.as_ref().map(ToString::to_string) == row.staged_release_key
            && i64::try_from(value.record_version)? == row.record_version,
        "acquisition document disagrees with selected identity or metadata"
    );
    Ok(value)
}

#[cfg(test)]
mod tests;
