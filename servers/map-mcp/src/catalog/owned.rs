//! Owner-scoped catalog pages and database-selected maintenance batches.
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use veoveo_platform_store::{MapDependencyKind, MapRouteState};

use super::{MapAccessContext, MapCatalog, decode, encode};
use crate::{
    contract::{
        AcquisitionId, AcquisitionJob, AcquisitionStatus, DatasetReleaseId, MapCatalogPage,
        MobilityProfileId, RestrictionId, RouteId, RouteMatrix, RouteMatrixId, RoutePlan,
        RouteStatus,
    },
    uris,
};

pub const PAGE_SIZE: usize = 100;

#[derive(Debug, Serialize)]
pub struct OwnedPage<T> {
    pub items: Vec<T>,
    pub limit: usize,
    pub next_cursor: Option<String>,
}

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

#[derive(Debug, Serialize)]
pub struct RouteSummary {
    pub route_id: RouteId,
    pub resource_uri: String,
    pub status: RouteStatus,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: crate::contract::MobilityProfileVersion,
    pub departure_time: DateTime<Utc>,
    pub arrival_time: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MatrixSummary {
    pub matrix_id: RouteMatrixId,
    pub resource_uri: String,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: crate::contract::MobilityProfileVersion,
    pub created_at: DateTime<Utc>,
}

impl MapCatalog {
    pub async fn route(&self, scope: &MapAccessContext, id: &RouteId) -> Result<Option<RoutePlan>> {
        self.store()
            .map_route(&scope.identity, id.as_str())
            .await?
            .map(|row| decode(&row.canonical_json, "route"))
            .transpose()
    }

    pub async fn matrix(
        &self,
        scope: &MapAccessContext,
        id: &RouteMatrixId,
    ) -> Result<Option<RouteMatrix>> {
        self.store()
            .map_route_matrix(&scope.identity, id.as_str())
            .await?
            .map(|row| {
                decode(
                    row.canonical_json
                        .as_deref()
                        .context("matrix document missing")?,
                    "route matrix",
                )
            })
            .transpose()
    }

    pub async fn acquisition(
        &self,
        scope: &MapAccessContext,
        id: &AcquisitionId,
    ) -> Result<Option<AcquisitionJob>> {
        self.store()
            .map_acquisition(&scope.identity, id.as_str())
            .await?
            .map(|row| decode(&row.canonical_json, "acquisition job"))
            .transpose()
    }

    pub async fn routes_page(
        &self,
        scope: &MapAccessContext,
        after: Option<&RouteId>,
    ) -> Result<OwnedPage<RouteSummary>> {
        let rows = self
            .store()
            .map_routes_page(&scope.identity, after.map(RouteId::as_str), PAGE_SIZE + 1)
            .await?;
        Collection::Routes.page(
            rows,
            |row| &row.route_key,
            |row| {
                Ok(RouteSummary {
                    resource_uri: crate::contract::MapRouteUri::new(row.route_key.parse()?)
                        .to_string(),
                    route_id: row.route_key.parse()?,
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
                })
            },
        )
    }

    pub async fn matrices_page(
        &self,
        scope: &MapAccessContext,
        after: Option<&RouteMatrixId>,
    ) -> Result<OwnedPage<MatrixSummary>> {
        let rows = self
            .store()
            .map_matrices_page(
                &scope.identity,
                after.map(RouteMatrixId::as_str),
                PAGE_SIZE + 1,
            )
            .await?;
        Collection::Matrices.page(
            rows,
            |row| &row.matrix_key,
            |row| {
                Ok(MatrixSummary {
                    resource_uri: uris::matrix_uri(&row.matrix_key.parse()?),
                    matrix_id: row.matrix_key.parse()?,
                    mobility_profile_id: row.mobility_profile_key.parse()?,
                    mobility_profile_version: row.mobility_profile_version.try_into()?,
                    created_at: row.created_at,
                })
            },
        )
    }

    pub async fn acquisitions_page(
        &self,
        scope: &MapAccessContext,
        after: Option<&AcquisitionId>,
    ) -> Result<OwnedPage<AcquisitionJob>> {
        let rows = self
            .store()
            .map_acquisitions_page(
                &scope.identity,
                after.map(AcquisitionId::as_str),
                PAGE_SIZE + 1,
            )
            .await?;
        Collection::Acquisitions.page(
            rows,
            |row| &row.acquisition_key,
            |row| decode(&row.canonical_json, "acquisition job"),
        )
    }

    async fn interrupted_acquisitions_batch(
        &self,
        scope: &MapAccessContext,
        active: &[AcquisitionId],
        after: Option<&AcquisitionId>,
    ) -> Result<Vec<AcquisitionJob>> {
        self.store()
            .map_interrupted_acquisitions_page(
                &scope.identity,
                &active.iter().map(ToString::to_string).collect::<Vec<_>>(),
                after.map(AcquisitionId::as_str),
                PAGE_SIZE,
            )
            .await?
            .into_iter()
            .map(|row| decode(&row.canonical_json, "acquisition job"))
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
        self.invalidate_routes(scope, MapDependencyKind::Release, id.as_str())
            .await
    }

    pub async fn invalidate_routes_for_restriction(
        &self,
        scope: &MapAccessContext,
        id: &RestrictionId,
    ) -> Result<u64> {
        self.invalidate_routes(scope, MapDependencyKind::Restriction, id.as_str())
            .await
    }

    async fn invalidate_routes(
        &self,
        scope: &MapAccessContext,
        kind: MapDependencyKind,
        dependency: &str,
    ) -> Result<u64> {
        let mut after = None;
        let mut count = 0;
        loop {
            let rows = self
                .store()
                .map_routes_for_dependency_page(
                    scope.identity.tenant_id,
                    kind,
                    dependency,
                    after.as_deref(),
                    PAGE_SIZE,
                )
                .await?;
            if rows.is_empty() {
                break;
            }
            after = rows.last().map(|row| row.route_key.clone());
            for row in rows {
                let mut route: RoutePlan = decode(&row.canonical_json, "route")?;
                route.status = RouteStatus::Invalidated;
                count += u64::from(
                    self.store()
                        .invalidate_map_route(
                            scope.identity.tenant_id,
                            route.route_id.as_str(),
                            encode(&route)?,
                        )
                        .await?,
                );
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests;
