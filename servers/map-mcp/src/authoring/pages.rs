//! Authoring metadata pages preserve SQL visibility and bind optional URI parents.
use anyhow::{Context, Result};
use serde::Serialize;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_types::AccessLevel;

use super::{
    AuthoringService,
    service::{decode, read_scope, require_access},
};
use crate::{
    catalog::MapAccessContext,
    contract::{
        FeatureLayer, FeatureLayerId, LayerProduct, LayerProductId, LayerPublication,
        LayerPublicationId, MapComposition, MapCompositionId, MapMetadataPage, MapMetadataRequest,
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
                let rows = self
                    .store()
                    .map_feature_layers_page(
                        &scope,
                        false,
                        after.as_ref().map(FeatureLayerId::as_str),
                        limit,
                    )
                    .await?;
                AuthoringPage::Layers(page(
                    rows,
                    |row| {
                        Ok(MapMetadataRequest::Layers {
                            after: Some(row.layer_key.parse()?),
                        })
                    },
                    |row| decode(&row.canonical_json, "feature layer"),
                )?)
            }
            MapMetadataRequest::Publications { layer, after } => {
                let rows = self
                    .store()
                    .map_layer_publications_page(
                        &scope,
                        layer.as_ref().map(FeatureLayerId::as_str),
                        after.as_ref().map(LayerPublicationId::as_str),
                        limit,
                    )
                    .await?;
                AuthoringPage::Publications(page(
                    rows,
                    |row| {
                        Ok(MapMetadataRequest::Publications {
                            layer: layer.clone(),
                            after: Some(row.publication_key.parse()?),
                        })
                    },
                    |row| decode(&row.canonical_json, "layer publication"),
                )?)
            }
            MapMetadataRequest::Products { publication, after } => {
                let rows = self
                    .store()
                    .map_layer_products_page(
                        &scope,
                        publication.as_ref().map(LayerPublicationId::as_str),
                        after.as_ref().map(LayerProductId::as_str),
                        limit,
                    )
                    .await?;
                AuthoringPage::Products(page(
                    rows,
                    |row| {
                        Ok(MapMetadataRequest::Products {
                            publication: publication.clone(),
                            after: Some(row.product_key.parse()?),
                        })
                    },
                    |row| decode(&row.canonical_json, "layer product"),
                )?)
            }
            MapMetadataRequest::Compositions { after } => {
                let rows = self
                    .store()
                    .map_compositions_page(
                        &scope,
                        false,
                        after.as_ref().map(MapCompositionId::as_str),
                        limit,
                    )
                    .await?;
                AuthoringPage::Compositions(page(
                    rows,
                    |row| {
                        Ok(MapMetadataRequest::Compositions {
                            after: Some(row.composition_key.parse()?),
                        })
                    },
                    |row| decode(&row.canonical_json, "map composition"),
                )?)
            }
        })
    }
}
