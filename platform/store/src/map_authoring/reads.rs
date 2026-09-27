//! Caller-scoped authoring reads. SQL owns label and parent-layer visibility.
mod completion;
pub use completion::MapAuthoringCompletion;

use surrealdb::types::{Array, RecordId};

use crate::{
    MapCompositionRecord, MapFeatureLayerRecord, MapLayerProductRecord, MapLayerPublicationRecord,
    PlatformStore, StoreError, deterministic_tenant_id, deterministic_work_context_id,
};

/// Constructed from authenticated identity after the service admits context access.
#[derive(Clone, Debug)]
pub struct MapAuthoringReadScope {
    tenant_key: String,
    tenant: RecordId,
    context: RecordId,
    labels: Vec<String>,
}
impl MapAuthoringReadScope {
    pub fn new(tenant: &str, context: &str, labels: Vec<String>) -> Result<Self, StoreError> {
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

const LAYER_VISIBLE: &str =
    "tenant = $tenant AND work_context = $context AND $labels CONTAINSALL data_labels";
const COMPOSITION_VISIBLE: &str =
    "tenant = $tenant AND work_context = $context AND $labels CONTAINSALL authority.data_labels";
// Publications and products inherit current layer access, including revocation by
// a changed label set. Selecting parents in SQL also handles removed layers.
const CHILD_VISIBLE: &str = "tenant = $tenant AND work_context = $context AND layer_key IN (SELECT VALUE layer_key FROM map_feature_layer WHERE tenant = $tenant AND work_context = $context AND $labels CONTAINSALL data_labels)";

impl PlatformStore {
    pub async fn map_feature_layer(
        &self,
        scope: &MapAuthoringReadScope,
        key: &str,
    ) -> Result<Option<MapFeatureLayerRecord>, StoreError> {
        let mut response = self
            .client()
            .query(format!("SELECT * FROM ONLY $record WHERE {LAYER_VISIBLE};"))
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
    ) -> Result<Vec<MapFeatureLayerRecord>, StoreError> {
        validate_page(after, "feature-layer-", limit)?;
        let archived = if include_archived {
            ""
        } else {
            " AND archived_at = NONE"
        };
        let mut response=self.client().query(format!("SELECT * FROM map_feature_layer WHERE {LAYER_VISIBLE}{archived} AND ($after = NONE OR layer_key > $after) ORDER BY layer_key ASC LIMIT $limit;"))
            .bind(("tenant",scope.tenant.clone())).bind(("context",scope.context.clone()))
            .bind(("labels",scope.labels.clone())).bind(("after",after.map(ToOwned::to_owned))).bind(("limit",limit)).await?.check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_composition(
        &self,
        scope: &MapAuthoringReadScope,
        key: &str,
    ) -> Result<Option<MapCompositionRecord>, StoreError> {
        let mut response = self
            .client()
            .query(format!(
                "SELECT * FROM ONLY $record WHERE {COMPOSITION_VISIBLE};"
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
    ) -> Result<Vec<MapCompositionRecord>, StoreError> {
        validate_page(after, "composition-", limit)?;
        let archived = if include_archived {
            ""
        } else {
            " AND archived_at = NONE"
        };
        let mut response=self.client().query(format!("SELECT * FROM map_composition WHERE {COMPOSITION_VISIBLE}{archived} AND ($after = NONE OR composition_key > $after) ORDER BY composition_key ASC LIMIT $limit;"))
            .bind(("tenant",scope.tenant.clone())).bind(("context",scope.context.clone()))
            .bind(("labels",scope.labels.clone())).bind(("after",after.map(ToOwned::to_owned))).bind(("limit",limit)).await?.check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_layer_publications_page(
        &self,
        scope: &MapAuthoringReadScope,
        layer: Option<&str>,
        after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MapLayerPublicationRecord>, StoreError> {
        validate_page(after, "publication-", limit)?;
        if let Some(layer) = layer {
            super::validate_key("layer", layer, "feature-layer-")?;
        }
        let parent = if layer.is_some() {
            " AND layer_key = $layer"
        } else {
            ""
        };
        let mut response=self.client().query(format!("SELECT * FROM map_layer_publication WHERE {CHILD_VISIBLE}{parent} AND ($after = NONE OR publication_key > $after) ORDER BY publication_key ASC LIMIT $limit;"))
            .bind(("tenant",scope.tenant.clone())).bind(("context",scope.context.clone())).bind(("labels",scope.labels.clone()))
            .bind(("layer",layer.map(ToOwned::to_owned))).bind(("after",after.map(ToOwned::to_owned))).bind(("limit",limit)).await?.check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_layer_product(
        &self,
        scope: &MapAuthoringReadScope,
        layer: &str,
        publication: &str,
        key: &str,
    ) -> Result<Option<MapLayerProductRecord>, StoreError> {
        let mut response = self
            .client()
            .query(format!("SELECT * FROM ONLY $record WHERE {CHILD_VISIBLE} AND layer_key = $layer AND publication_key = $publication;"))
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
    ) -> Result<Vec<MapLayerProductRecord>, StoreError> {
        validate_page(after, "layer-product-", limit)?;
        if let Some(publication) = publication {
            super::validate_key("publication", publication, "publication-")?;
        }
        let parent = if publication.is_some() {
            " AND publication_key = $publication"
        } else {
            ""
        };
        let mut response=self.client().query(format!("SELECT * FROM map_layer_product WHERE {CHILD_VISIBLE}{parent} AND ($after = NONE OR product_key > $after) ORDER BY product_key ASC LIMIT $limit;"))
            .bind(("tenant",scope.tenant.clone())).bind(("context",scope.context.clone())).bind(("labels",scope.labels.clone()))
            .bind(("publication",publication.map(ToOwned::to_owned))).bind(("after",after.map(ToOwned::to_owned))).bind(("limit",limit)).await?.check()?;
        Ok(response.take(0)?)
    }
}

fn validate_page(after: Option<&str>, prefix: &str, limit: usize) -> Result<(), StoreError> {
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
