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
    pub(crate) fn tenant_key(&self) -> &str {
        &self.tenant_key
    }
    pub fn new(tenant: &str, context: &str, labels: Vec<String>) -> Result<Self, MapStoreError> {
        Ok(Self {
            tenant_key: tenant.to_owned(),
            tenant: deterministic_tenant_id(tenant)?.record_id(),
            context: deterministic_work_context_id(tenant, context)?.record_id(),
            labels,
        })
    }
    fn record(&self, table: &str, key: impl AsRef<str>) -> RecordId {
        RecordId::new(
            table,
            Array::from(vec![self.tenant_key.clone(), key.as_ref().to_owned()]),
        )
    }
}

impl MapRepository {
    /// Parent policy, receipt and retained revisions share one current read snapshot.
    pub(crate) async fn retained_map_feature_commit(
        &self,
        scope: &MapAuthoringReadScope,
        layer: &crate::contract::FeatureLayerId,
        changeset: &crate::contract::FeatureChangeSetId,
        digest: &str,
        idempotency_key: &str,
    ) -> anyhow::Result<Option<super::MapFeatureCommitResult>> {
        let scope = scope.clone();
        let parent = scope.record("map_feature_layer", layer);
        let receipt = super::authored_record(
            "map_feature_changeset",
            &scope.tenant_key,
            &[layer.as_str(), changeset.as_str()],
        );
        let layer = layer.clone();
        let changeset = changeset.clone();
        let digest = digest.to_owned();
        let idempotency_key = idempotency_key.to_owned();
        veoveo_platform_store::read_transaction::read(self.client(), move |transaction| {
            Box::pin(async move {
                let mut response = transaction
                    .query(include_str!(
                        "../queries/map_authoring/reads/retained_feature_commit_parent.surql"
                    ))
                    .bind(("record", parent))
                    .bind(("tenant", scope.tenant.clone()))
                    .bind(("context", scope.context.clone()))
                    .bind(("labels", scope.labels))
                    .await?
                    .check()?;
                let admitted: Option<RecordId> = response.take(0)?;
                anyhow::ensure!(
                    admitted.is_some(),
                    "current feature layer policy denies retained commit access"
                );
                let mut response = transaction
                    .query(include_str!("../queries/map_authoring/select_scoped.surql"))
                    .bind(("record", receipt))
                    .bind(("tenant", scope.tenant.clone()))
                    .bind(("context", scope.context.clone()))
                    .await?
                    .check()?;
                let Some(changeset_row): Option<crate::persistence::MapFeatureChangeSetRecord> =
                    response.take(0)?
                else {
                    return Ok(None);
                };
                anyhow::ensure!(
                    changeset_row.layer_key == layer.as_str()
                        && changeset_row.changeset_key == changeset.as_str(),
                    "retained commit identity disagrees with the selected layer and changeset"
                );
                anyhow::ensure!(
                    changeset_row.request_digest_sha256 == digest,
                    "feature changeset idempotency key conflicts with the retained request"
                );
                anyhow::ensure!(
                    changeset_row.idempotency_key == idempotency_key,
                    "feature changeset idempotency identity disagrees"
                );
                let mut response = transaction
                    .query(include_str!(
                        "../queries/map_authoring/list_map_feature_revisions_for_changeset.surql"
                    ))
                    .bind(("tenant", scope.tenant))
                    .bind(("context", scope.context))
                    .bind(("changeset", changeset.to_string()))
                    .await?
                    .check()?;
                let revisions: Vec<crate::persistence::MapFeatureRevisionRecord> =
                    response.take(0)?;
                let feature_keys: std::collections::BTreeSet<_> = revisions
                    .iter()
                    .map(|row| row.feature_key.as_str())
                    .collect();
                let expected: std::collections::BTreeSet<_> = changeset_row
                    .feature_keys
                    .iter()
                    .map(String::as_str)
                    .collect();
                anyhow::ensure!(
                    feature_keys == expected
                        && revisions.len() == expected.len()
                        && revisions.iter().all(|row| row.layer_key == layer.as_str()
                            && row.changeset_key == changeset.as_str()),
                    "retained commit feature revisions disagree with its membership"
                );
                Ok(Some(super::MapFeatureCommitResult {
                    changeset: changeset_row,
                    revisions,
                }))
            })
        })
        .await
    }

    pub async fn map_feature_layer(
        &self,
        scope: &MapAuthoringReadScope,
        key: &crate::contract::FeatureLayerId,
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
        after: Option<&crate::contract::FeatureLayerId>,
        limit: usize,
    ) -> Result<Vec<MapFeatureLayerRecord>, MapStoreError> {
        validate_page(limit)?;
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
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_composition(
        &self,
        scope: &MapAuthoringReadScope,
        key: &crate::contract::MapCompositionId,
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
        after: Option<&crate::contract::MapCompositionId>,
        limit: usize,
    ) -> Result<Vec<MapCompositionRecord>, MapStoreError> {
        validate_page(limit)?;
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
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_layer_publications_page(
        &self,
        scope: &MapAuthoringReadScope,
        layer: Option<&crate::contract::FeatureLayerId>,
        after: Option<&crate::contract::LayerPublicationId>,
        limit: usize,
    ) -> Result<Vec<MapLayerPublicationRecord>, MapStoreError> {
        validate_page(limit)?;
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
            .bind(("layer", layer.map(ToString::to_string)))
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
    pub async fn map_layer_product(
        &self,
        scope: &MapAuthoringReadScope,
        layer: &crate::contract::FeatureLayerId,
        publication: &crate::contract::LayerPublicationId,
        key: &crate::contract::LayerProductId,
    ) -> Result<Option<MapLayerProductRecord>, MapStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/map_authoring/reads/map_layer_product.surql"
            ))
            .bind(("record", scope.record("map_layer_product", key)))
            .bind(("layer", layer.to_string()))
            .bind(("publication", publication.to_string()))
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
        publication: Option<&crate::contract::LayerPublicationId>,
        after: Option<&crate::contract::LayerProductId>,
        limit: usize,
    ) -> Result<Vec<MapLayerProductRecord>, MapStoreError> {
        validate_page(limit)?;
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
            .bind(("publication", publication.map(ToString::to_string)))
            .bind(("after", after.map(ToString::to_string)))
            .bind(("limit", limit))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}

fn validate_page(limit: usize) -> Result<(), MapStoreError> {
    if !(1..=101).contains(&limit) {
        return Err(super::invalid(
            "limit",
            "Map metadata pages require 1..=101 rows",
        ));
    }
    Ok(())
}
