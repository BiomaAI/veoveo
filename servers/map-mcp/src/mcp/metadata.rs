//! Authored resources and metadata pages share current SQL visibility.
use super::*;
use crate::contract::{MapMetadataRequest, MapResource};

impl MapMcp {
    pub(super) async fn read_authoring_page(
        &self,
        request: MapMetadataRequest,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(context, MapScope::FeatureRead)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let page = self
            .state
            .authoring
            .metadata_page(&identity, &scope, request)
            .await
            .map_err(internal)?;
        json_read(uri, &page)
    }

    pub(super) async fn read_authoring_resource(
        &self,
        address: MapResource,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(context, MapScope::FeatureRead)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match address {
            MapResource::Layer { layer } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .layer(&identity, &scope, &layer)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("feature layer"))?,
            ),
            MapResource::Schema { layer, version } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .schema_revision(&identity, &scope, &layer, version)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("feature schema revision"))?,
            ),
            MapResource::Style { layer, version } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .style_revision(&identity, &scope, &layer, version)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("feature style revision"))?,
            ),
            MapResource::StyleRevision { id } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .style_revision_by_id(&identity, &scope, &id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("feature style revision"))?,
            ),
            MapResource::Feature { layer, feature } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .feature(&identity, &scope, &layer, &feature)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("feature"))?,
            ),
            MapResource::FeatureRevision {
                layer,
                feature,
                revision,
            } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .feature_revision(&identity, &scope, &layer, &feature, revision)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("feature revision"))?,
            ),
            MapResource::Changeset { layer, changeset } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .changeset(&identity, &scope, &layer, &changeset)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("feature changeset"))?,
            ),
            MapResource::Publication { layer, publication } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .publication(&identity, &scope, &layer, &publication)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("layer publication"))?,
            ),
            MapResource::Product {
                layer,
                publication,
                product,
            } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .layer_product(&identity, &scope, &layer, &publication, &product)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("map layer product"))?,
            ),
            MapResource::Composition { id } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .composition(&identity, &scope, &id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("map composition"))?,
            ),
            MapResource::CompositionRevision { id, revision } => json_read(
                uri,
                &self
                    .state
                    .authoring
                    .composition_revision(&identity, &scope, &id, revision)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("map composition revision"))?,
            ),
            _ => Err(internal("expected an authored Map resource")),
        }
    }
}
