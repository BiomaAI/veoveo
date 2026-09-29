//! Shared derivation persistence and lightweight cursor pages.
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use veoveo_platform_store::{MapDerivationDraft, MapDerivationKind, MapDerivationScope};
use veoveo_types::{PrincipalId, WorkContextId};

use crate::{
    catalog::{MapAccessContext, MapCatalog},
    contract::{RasterDerivation, RasterDerivationId, SpatialDerivation, SpatialDerivationId},
};
mod migration;
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
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    kind: MapDerivationKind,
    after: String,
}

pub fn parse_cursor(kind: MapDerivationKind, cursor: Option<&str>) -> Result<Option<String>> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    ensure!(
        !cursor.is_empty() && cursor.len() <= 2048,
        "invalid Map derivation cursor"
    );
    let cursor: Cursor = serde_json::from_slice(&hex::decode(cursor)?)?;
    ensure!(
        cursor.version == 1 && cursor.kind == kind,
        "invalid Map derivation cursor"
    );
    match kind {
        MapDerivationKind::Raster => {
            RasterDerivationId::parse(&cursor.after)?;
        }
        MapDerivationKind::Spatial => {
            SpatialDerivationId::parse(&cursor.after)?;
        }
    }
    Ok(Some(cursor.after))
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
        self.store()
            .put_map_derivation(MapDerivationDraft {
                scope: self::scope(scope, &derivation.work_context)?,
                kind: MapDerivationKind::Raster,
                derivation_key: derivation.derivation_id.to_string(),
                created_by: derivation.created_by.to_string(),
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
        self.store()
            .put_map_derivation(MapDerivationDraft {
                scope: self::scope(scope, &derivation.work_context)?,
                kind: MapDerivationKind::Spatial,
                derivation_key: derivation.derivation_id.to_string(),
                created_by: derivation.created_by.to_string(),
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
        let row = self
            .store()
            .map_derivation(
                self::scope(scope, context)?,
                MapDerivationKind::Raster,
                id.as_str(),
            )
            .await?;
        row.map(|row| {
            let value: RasterDerivation = serde_json::from_str(&row.canonical_json)?;
            value.validate()?;
            ensure!(
                value.derivation_id == *id && value.work_context == *context,
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
        let row = self
            .store()
            .map_derivation(
                self::scope(scope, context)?,
                MapDerivationKind::Spatial,
                id.as_str(),
            )
            .await?;
        row.map(|row| {
            let value: SpatialDerivation = serde_json::from_str(&row.canonical_json)?;
            value.validate()?;
            ensure!(
                value.derivation_id == *id && value.work_context == *context,
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
        kind: MapDerivationKind,
        after: Option<&str>,
    ) -> Result<DerivationPage> {
        let mut rows = self
            .store()
            .map_derivations_page(self::scope(scope, context)?, kind, after, PAGE_SIZE + 1)
            .await?;
        let more = rows.len() > PAGE_SIZE;
        rows.truncate(PAGE_SIZE);
        let next_cursor = if more {
            Some(hex::encode(serde_json::to_vec(&Cursor {
                version: 1,
                kind,
                after: rows.last().expect("nonempty page").derivation_key.clone(),
            })?))
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
                    created_by: PrincipalId::new(row.created_by)?,
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
        Ok(self
            .store()
            .complete_map_derivations(self::scope(scope, context)?, kind, needle)
            .await?)
    }
}
