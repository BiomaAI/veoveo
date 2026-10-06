//! Authoring metadata pages preserve SQL visibility and bind optional URI parents.
use crate::persistence::MapRepository;
use anyhow::{Context, Result};
use serde::Serialize;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_types::AccessLevel;

use super::{
    AuthoringService,
    service::{read_scope, require_access},
};
use crate::{
    catalog::MapAccessContext,
    contract::{
        FeatureLayer, LayerProduct, LayerPublication, MapComposition, MapMetadataPage,
        MapMetadataRequest,
    },
};

pub const PAGE_SIZE: usize = 100;

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum AuthoringPage {
    Layers(MapMetadataPage<FeatureLayer>),
    Publications(MapMetadataPage<LayerPublication>),
    Products(MapMetadataPage<LayerProduct>),
    Compositions(MapMetadataPage<MapComposition>),
}

fn page<T, U>(
    mut rows: Vec<T>,
    resume: impl Fn(&T) -> Result<MapMetadataRequest>,
    convert: impl Fn(T) -> Result<U>,
) -> Result<MapMetadataPage<U>> {
    let more = rows.len() > PAGE_SIZE;
    rows.truncate(PAGE_SIZE);
    let next_cursor = if more {
        Some(
            resume(rows.last().context("missing Map page cursor")?)?
                .cursor()
                .context("missing Map page position")?,
        )
    } else {
        None
    };
    Ok(MapMetadataPage {
        items: rows.into_iter().map(convert).collect::<Result<_>>()?,
        limit: PAGE_SIZE,
        next_cursor,
    })
}

impl AuthoringService {
    pub async fn metadata_page(
        &self,
        identity: &GatewayInternalIdentity,
        scope: &MapAccessContext,
        request: MapMetadataRequest,
    ) -> Result<AuthoringPage> {
        require_access(identity, AccessLevel::Read)?;
        let scope = read_scope(identity, scope)?;
        let limit = PAGE_SIZE + 1;
        Ok(match &request {
            MapMetadataRequest::Layers { after } => {
                let rows = MapRepository::new(self.store().clone())
                    .map_feature_layers_page(&scope, false, after.as_ref(), limit)
                    .await?;
                AuthoringPage::Layers(page(
                    rows,
                    |row| {
                        Ok(MapMetadataRequest::Layers {
                            after: Some(row.layer_key.parse()?),
                        })
                    },
                    |row| super::hydration::layer(row, scope.tenant_key()),
                )?)
            }
            MapMetadataRequest::Publications { layer, after } => {
                let rows = MapRepository::new(self.store().clone())
                    .map_layer_publications_page(&scope, layer.as_ref(), after.as_ref(), limit)
                    .await?;
                AuthoringPage::Publications(page(
                    rows,
                    |row| {
                        Ok(MapMetadataRequest::Publications {
                            layer: layer.clone(),
                            after: Some(row.publication_key.parse()?),
                        })
                    },
                    |row| super::hydration::publication(row, scope.tenant_key()),
                )?)
            }
            MapMetadataRequest::Products { publication, after } => {
                let rows = MapRepository::new(self.store().clone())
                    .map_layer_products_page(&scope, publication.as_ref(), after.as_ref(), limit)
                    .await?;
                AuthoringPage::Products(page(
                    rows,
                    |row| {
                        Ok(MapMetadataRequest::Products {
                            publication: publication.clone(),
                            after: Some(row.product_key.parse()?),
                        })
                    },
                    |row| super::hydration::product(row, scope.tenant_key()),
                )?)
            }
            MapMetadataRequest::Compositions { after } => {
                let rows = MapRepository::new(self.store().clone())
                    .map_compositions_page(&scope, false, after.as_ref(), limit)
                    .await?;
                AuthoringPage::Compositions(page(
                    rows,
                    |row| {
                        Ok(MapMetadataRequest::Compositions {
                            after: Some(row.composition_key.parse()?),
                        })
                    },
                    |row| super::hydration::composition(row, scope.tenant_key()),
                )?)
            }
        })
    }
}
