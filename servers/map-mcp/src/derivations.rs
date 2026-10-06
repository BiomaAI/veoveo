//! Shared derivation persistence and lightweight cursor pages.
use crate::persistence::MapRepository;
use crate::persistence::{
    MapDerivationDraft, MapDerivationIdentity, MapDerivationKind, MapDerivationScope,
};
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_types::{PrincipalId, WorkContextId};

use crate::{
    catalog::{MapAccessContext, MapCatalog},
    contract::{
        MapCatalogPage, RasterDerivation, RasterDerivationId, SpatialDerivation,
        SpatialDerivationId,
    },
};
#[cfg(test)]
mod tests;

pub const PAGE_SIZE: usize = 100;
#[derive(Debug, Serialize)]
pub struct DerivationPage {
    pub items: Vec<DerivationSummary>,
    pub limit: usize,
    pub next_cursor: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct DerivationSummary {
    pub derivation_id: String,
    pub resource_uri: String,
    pub created_by: PrincipalId,
    pub created_at: DateTime<Utc>,
}
pub enum DerivationSelection<'a> {
    Raster(Option<&'a RasterDerivationId>),
    Spatial(Option<&'a SpatialDerivationId>),
}

fn scope(scope: &MapAccessContext, context: &WorkContextId) -> Result<MapDerivationScope> {
    Ok(MapDerivationScope::from_keys(
        &scope.identity.tenant_key,
        context.as_str(),
    )?)
}

impl MapCatalog {
    pub async fn put_raster_derivation(
        &self,
        scope: &MapAccessContext,
        derivation: &RasterDerivation,
    ) -> Result<()> {
        derivation.validate()?;
        MapRepository::new(self.store().clone())
            .put_map_derivation(MapDerivationDraft {
                scope: self::scope(scope, &derivation.work_context)?,
                identity: MapDerivationIdentity::Raster(derivation.derivation_id.clone()),
                created_by: derivation.created_by.clone(),
                created_at: derivation.created_at,
                canonical_json: serde_json::to_string(derivation)?,
            })
            .await?;
        Ok(())
    }
    pub async fn put_spatial_derivation(
        &self,
        scope: &MapAccessContext,
        derivation: &SpatialDerivation,
    ) -> Result<()> {
        derivation.validate()?;
        MapRepository::new(self.store().clone())
            .put_map_derivation(MapDerivationDraft {
                scope: self::scope(scope, &derivation.work_context)?,
                identity: MapDerivationIdentity::Spatial(derivation.derivation_id.clone()),
                created_by: derivation.created_by.clone(),
                created_at: derivation.created_at,
                canonical_json: serde_json::to_string(derivation)?,
            })
            .await?;
        Ok(())
    }
    pub async fn raster_derivation(
        &self,
        scope: &MapAccessContext,
        context: &WorkContextId,
        id: &RasterDerivationId,
    ) -> Result<Option<RasterDerivation>> {
        let row = MapRepository::new(self.store().clone())
            .map_derivation(
                self::scope(scope, context)?,
                &MapDerivationIdentity::Raster(id.clone()),
            )
            .await?;
        row.map(|row| {
            let value: RasterDerivation = serde_json::from_str(&row.canonical_json)?;
            value.validate()?;
            ensure!(
                value.derivation_id == *id
                    && value.work_context == *context
                    && value.created_by.as_str() == row.created_by
                    && value.created_at == row.created_at,
                "stored raster derivation identity mismatch"
            );
            Ok(value)
        })
        .transpose()
    }
    pub async fn spatial_derivation(
        &self,
        scope: &MapAccessContext,
        context: &WorkContextId,
        id: &SpatialDerivationId,
    ) -> Result<Option<SpatialDerivation>> {
        let row = MapRepository::new(self.store().clone())
            .map_derivation(
                self::scope(scope, context)?,
                &MapDerivationIdentity::Spatial(id.clone()),
            )
            .await?;
        row.map(|row| {
            let value: SpatialDerivation = serde_json::from_str(&row.canonical_json)?;
            value.validate()?;
            ensure!(
                value.derivation_id == *id
                    && value.work_context == *context
                    && value.created_by.as_str() == row.created_by
                    && value.created_at == row.created_at,
                "stored spatial derivation identity mismatch"
            );
            Ok(value)
        })
        .transpose()
    }
    pub async fn derivations_page(
        &self,
        scope: &MapAccessContext,
        context: &WorkContextId,
        selection: DerivationSelection<'_>,
    ) -> Result<DerivationPage> {
        let (kind, after) = match selection {
            DerivationSelection::Raster(after) => (
                MapDerivationKind::Raster,
                after.cloned().map(MapDerivationIdentity::Raster),
            ),
            DerivationSelection::Spatial(after) => (
                MapDerivationKind::Spatial,
                after.cloned().map(MapDerivationIdentity::Spatial),
            ),
        };
        let mut rows = MapRepository::new(self.store().clone())
            .map_derivations_page(
                self::scope(scope, context)?,
                kind,
                after.as_ref(),
                PAGE_SIZE + 1,
            )
            .await?;
        for row in &rows {
            let agrees = match kind {
                MapDerivationKind::Raster => {
                    let value: RasterDerivation = serde_json::from_str(&row.canonical_json)?;
                    value.derivation_id.as_str() == row.derivation_key
                        && value.work_context == *context
                        && value.created_by.as_str() == row.created_by
                        && value.created_at == row.created_at
                }
                MapDerivationKind::Spatial => {
                    let value: SpatialDerivation = serde_json::from_str(&row.canonical_json)?;
                    value.derivation_id.as_str() == row.derivation_key
                        && value.work_context == *context
                        && value.created_by.as_str() == row.created_by
                        && value.created_at == row.created_at
                }
            };
            ensure!(
                agrees,
                "derivation document disagrees with selected metadata"
            );
        }
        let more = rows.len() > PAGE_SIZE;
        rows.truncate(PAGE_SIZE);
        let next_cursor = if more {
            let key = &rows.last().expect("nonempty page").derivation_key;
            match kind {
                MapDerivationKind::Raster => MapCatalogPage::RasterDerivations {
                    after: Some(key.parse()?),
                },
                MapDerivationKind::Spatial => MapCatalogPage::SpatialDerivations {
                    after: Some(key.parse()?),
                },
            }
            .cursor()
        } else {
            None
        };
        let items = rows
            .into_iter()
            .map(|row| {
                let resource_uri = match kind {
                    MapDerivationKind::Raster => {
                        crate::contract::MapRasterDerivationUri::new(row.derivation_key.parse()?)
                            .to_string()
                    }
                    MapDerivationKind::Spatial => {
                        crate::contract::MapSpatialDerivationUri::new(row.derivation_key.parse()?)
                            .to_string()
                    }
                };
                Ok(DerivationSummary {
                    derivation_id: row.derivation_key,
                    resource_uri,
                    created_by: PrincipalId::parse(row.created_by)?,
                    created_at: row.created_at,
                })
            })
            .collect::<Result<_>>()?;
        Ok(DerivationPage {
            items,
            limit: PAGE_SIZE,
            next_cursor,
        })
    }
    pub async fn complete_derivations(
        &self,
        scope: &MapAccessContext,
        context: &WorkContextId,
        kind: MapDerivationKind,
        needle: &str,
    ) -> Result<Vec<String>> {
        Ok(MapRepository::new(self.store().clone())
            .complete_map_derivations(self::scope(scope, context)?, kind, needle)
            .await?)
    }
}
