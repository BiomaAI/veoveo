//! Caller-scoped authoring reads. SQL owns label and parent-layer visibility.
mod completion;
pub use completion::MapAuthoringCompletion;

use surrealdb::types::{Array, RecordId};

use crate::persistence::{
    MapCompositionRecord, MapFeatureLayerRecord, MapLayerProductRecord, MapLayerPublicationRecord,
    MapRepository, MapStoreError,
};
use veoveo_platform_store::{deterministic_tenant_id, deterministic_work_context_id};

/// Constructed from authenticated identity after the service admits context access.
#[derive(Clone, Debug)]
pub struct MapAuthoringReadScope {
    tenant_key: String,
    tenant: RecordId,
    context: RecordId,
    labels: Vec<String>,
}
impl MapAuthoringReadScope {
    pub fn new(tenant: &str, context: &str, labels: Vec<String>) -> Result<Self, MapStoreError> {
        Ok(Self {
            tenant_key: tenant.to_owned(),
            tenant: deterministic_tenant_id(tenant)?.record_id(),
            context: deterministic_work_context_id(tenant, context)?.record_id(),
            labels,
        })
    }
    fn record(&self, table: &str, key: &str) -> RecordId {
        RecordId::new(
            table,
            Array::from(vec![self.tenant_key.clone(), key.to_owned()]),
        )
    }
}

impl MapRepository {
    pub async fn map_feature_layer(
        &self,
        scope: &MapAuthoringReadScope,
        key: &str,
    ) -> Result<Option<MapFeatureLayerRecord>, MapStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map_authoring/reads/map_feature_layer.surql"
            ))
            .bind(("record", scope.record("map_feature_layer", key)))
            .bind(("tenant", scope.tenant.clone()))
            .bind(("context", scope.context.clone()))
            .bind(("labels", scope.labels.clone()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_feature_layers_page(
        &self,
        scope: &MapAuthoringReadScope,
        include_archived: bool,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapFeatureLayerRecord>, MapStoreError> {
        validate_page(after, "feature-layer-", limit)?;
        let sql = if include_archived {
            include_str!("../queries/map_authoring/reads/map_feature_layers_page_all.surql")
        } else {
            include_str!("../queries/map_authoring/reads/map_feature_layers_page_filtered.surql")
        };
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", scope.tenant.clone()))
            .bind(("context", scope.context.clone()))
            .bind(("labels", scope.labels.clone()))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_composition(
        &self,
        scope: &MapAuthoringReadScope,
        key: &str,
    ) -> Result<Option<MapCompositionRecord>, MapStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map_authoring/reads/map_composition.surql"
            ))
            .bind(("record", scope.record("map_composition", key)))
            .bind(("tenant", scope.tenant.clone()))
            .bind(("context", scope.context.clone()))
            .bind(("labels", scope.labels.clone()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_compositions_page(
        &self,
        scope: &MapAuthoringReadScope,
        include_archived: bool,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapCompositionRecord>, MapStoreError> {
        validate_page(after, "composition-", limit)?;
        let sql = if include_archived {
            include_str!("../queries/map_authoring/reads/map_compositions_page_all.surql")
        } else {
            include_str!("../queries/map_authoring/reads/map_compositions_page_filtered.surql")
        };
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", scope.tenant.clone()))
            .bind(("context", scope.context.clone()))
            .bind(("labels", scope.labels.clone()))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_layer_publications_page(
        &self,
        scope: &MapAuthoringReadScope,
        layer: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapLayerPublicationRecord>, MapStoreError> {
        validate_page(after, "publication-", limit)?;
        if let Some(layer) = layer {
            super::validate_key("layer", layer, "feature-layer-")?;
        }
        let sql = if layer.is_none() {
            include_str!("../queries/map_authoring/reads/map_layer_publications_page_all.surql")
        } else {
            include_str!(
                "../queries/map_authoring/reads/map_layer_publications_page_filtered.surql"
            )
        };
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", scope.tenant.clone()))
            .bind(("context", scope.context.clone()))
            .bind(("labels", scope.labels.clone()))
            .bind(("layer", layer.map(ToOwned::to_owned)))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_layer_product(
        &self,
        scope: &MapAuthoringReadScope,
        layer: &str,
        publication: &str,
        key: &str,
    ) -> Result<Option<MapLayerProductRecord>, MapStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map_authoring/reads/map_layer_product.surql"
            ))
            .bind(("record", scope.record("map_layer_product", key)))
            .bind(("layer", layer.to_owned()))
            .bind(("publication", publication.to_owned()))
            .bind(("tenant", scope.tenant.clone()))
            .bind(("context", scope.context.clone()))
            .bind(("labels", scope.labels.clone()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_layer_products_page(
        &self,
        scope: &MapAuthoringReadScope,
        publication: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapLayerProductRecord>, MapStoreError> {
        validate_page(after, "layer-product-", limit)?;
        if let Some(publication) = publication {
            super::validate_key("publication", publication, "publication-")?;
        }
        let sql = if publication.is_none() {
            include_str!("../queries/map_authoring/reads/map_layer_products_page_all.surql")
        } else {
            include_str!("../queries/map_authoring/reads/map_layer_products_page_filtered.surql")
        };
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", scope.tenant.clone()))
            .bind(("context", scope.context.clone()))
            .bind(("labels", scope.labels.clone()))
            .bind(("publication", publication.map(ToOwned::to_owned)))
            .bind(("after", after.map(ToOwned::to_owned)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}

fn validate_page(after: Option<&str>, prefix: &str, limit: usize) -> Result<(), MapStoreError> {
    if !(1..=101).contains(&limit) {
        return Err(super::invalid(
            "limit",
            "Map metadata pages require 1..=101 rows",
        ));
    }
    if let Some(after) = after {
        super::validate_key("after", after, prefix)?;
    }
    Ok(())
}
