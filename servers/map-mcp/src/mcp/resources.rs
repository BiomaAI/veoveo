//! Resource dispatch consumes the identities admitted by the shared host.
use super::*;
use crate::contract::{MapDatasetAddress, MapResource, MapRoot, MapTarget};

impl MapMcp {
    pub(super) async fn read_map_resource(
        &self,
        address: MapTarget,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        match address {
            MapTarget::KnowledgePage(address) => {
                self.read_knowledge_page(address, uri, context).await
            }
            MapTarget::KnowledgeMember(address) => {
                self.read_knowledge_member(address, context).await
            }
            MapTarget::Catalog(page) => self.read_catalog_page(page, uri, context).await,
            MapTarget::Metadata(request) => self.read_authoring_page(request, uri, context).await,
            MapTarget::Features(request) => {
                let identity = require_scope(context, MapScope::FeatureRead)?;
                let scope = self.state.scope(&identity).await.map_err(internal)?;
                let output = self
                    .state
                    .authoring
                    .query_features(&identity, &scope, request)
                    .await
                    .map_err(invalid_params)?;
                json_read(uri, &output)
            }
            MapTarget::Artifact(id) => {
                require_any_scope(context, &[MapScope::DatasetRead, MapScope::FeatureRead])?;
                let artifact = self
                    .state
                    .artifacts
                    .get(&plane_caller(context)?, &id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("artifact"))?;
                let content = ResourceContents::blob(BASE64_STANDARD.encode(&artifact.bytes), uri)
                    .with_mime_type(
                        artifact
                            .metadata
                            .mime_type
                            .unwrap_or_else(|| "application/octet-stream".to_owned()),
                    );
                Ok(ReadResourceResult::new(vec![content]))
            }
            MapTarget::Dataset(address) => self.read_dataset_resource(address, uri, context).await,
            MapTarget::Resource(address) => self.read_direct_resource(address, uri, context).await,
        }
    }

    async fn read_direct_resource(
        &self,
        address: MapResource,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        match address {
            MapResource::Document(_) | MapResource::Root(MapRoot::Docs | MapRoot::Contract) => {
                Err(served_by_host())
            }
            MapResource::WorkspaceApp => {
                require_any_scope(
                    context,
                    &[
                        MapScope::Admin,
                        MapScope::DatasetRead,
                        MapScope::FeatureRead,
                    ],
                )?;
                Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(uri, self.workspace_app.as_str()),
                ]))
            }
            MapResource::Root(MapRoot::Workspace) => {
                let identity = require_any_scope(
                    context,
                    &[
                        MapScope::Admin,
                        MapScope::DatasetRead,
                        MapScope::FeatureRead,
                    ],
                )?;
                json_read(
                    uri,
                    &crate::contract::MapWorkspaceAccess {
                        administration: identity_has_scope(&identity, MapScope::Admin),
                        dataset_read: identity_has_scope(&identity, MapScope::DatasetRead),
                        feature_read: identity_has_scope(&identity, MapScope::FeatureRead),
                        feature_write: identity_has_scope(&identity, MapScope::FeatureWrite),
                        feature_publish: identity_has_scope(&identity, MapScope::FeaturePublish),
                        basemap: self.state.workspace_basemap.clone(),
                    },
                )
            }
            MapResource::Acquisition { id } => {
                let identity = require_scope(context, MapScope::Admin)?;
                let scope = self.state.scope(&identity).await.map_err(internal)?;
                let job = self
                    .state
                    .catalog
                    .acquisition(&scope, &id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("acquisition"))?;
                json_read(uri, &job)
            }
            MapResource::Root(MapRoot::ActiveReleases) => {
                let identity =
                    require_any_scope(context, &[MapScope::Admin, MapScope::DatasetRead])?;
                let scope = self.state.scope(&identity).await.map_err(internal)?;
                json_read(
                    uri,
                    &self
                        .state
                        .catalog
                        .list_active_releases(&scope)
                        .await
                        .map_err(internal)?,
                )
            }
            MapResource::Root(
                root @ (MapRoot::Locations | MapRoot::Facilities | MapRoot::Rasters),
            ) => {
                let identity = require_scope(context, MapScope::DatasetRead)?;
                let scope = self.state.scope(&identity).await.map_err(internal)?;
                match root {
                    MapRoot::Locations => json_read(
                        uri,
                        &self
                            .state
                            .analytics
                            .list_locations(&scope.tenant_key(), 10_000)
                            .map_err(internal)?,
                    ),
                    MapRoot::Facilities => json_read(
                        uri,
                        &self
                            .state
                            .analytics
                            .list_facilities(&scope.tenant_key(), 10_000)
                            .map_err(internal)?,
                    ),
                    MapRoot::Rasters => json_read(
                        uri,
                        &self
                            .state
                            .analytics
                            .list_raster_products(&scope.tenant_key(), None, 10_000)
                            .map_err(internal)?,
                    ),
                    _ => unreachable!("selected geographic root"),
                }
            }
            MapResource::Location { id } => {
                let identity = require_scope(context, MapScope::DatasetRead)?;
                let scope = self.state.scope(&identity).await.map_err(internal)?;
                json_read(
                    uri,
                    &self
                        .state
                        .analytics
                        .location(&scope.tenant_key(), &id)
                        .map_err(internal)?
                        .ok_or_else(|| not_found("location"))?,
                )
            }
            MapResource::Facility { id } => {
                let identity = require_scope(context, MapScope::DatasetRead)?;
                let scope = self.state.scope(&identity).await.map_err(internal)?;
                json_read(
                    uri,
                    &self
                        .state
                        .analytics
                        .facility(&scope.tenant_key(), &id)
                        .map_err(internal)?
                        .ok_or_else(|| not_found("facility"))?,
                )
            }
            MapResource::Matrix { id } => {
                let identity = require_scope(context, MapScope::DatasetRead)?;
                let scope = self.state.scope(&identity).await.map_err(internal)?;
                json_read(
                    uri,
                    &self
                        .state
                        .catalog
                        .matrix(&scope, &id)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("matrix"))?,
                )
            }
            MapResource::Dataset { .. }
            | MapResource::Features { .. }
            | MapResource::Root(
                MapRoot::Datasets
                | MapRoot::Routes
                | MapRoot::Matrices
                | MapRoot::Acquisitions
                | MapRoot::RasterDerivations
                | MapRoot::SpatialDerivations
                | MapRoot::FeatureLayers
                | MapRoot::Publications
                | MapRoot::LayerProducts
                | MapRoot::Compositions
                | MapRoot::Sources
                | MapRoot::MobilityProfiles
                | MapRoot::Restrictions
                | MapRoot::TravelModels,
            ) => Err(internal("Map address was not normalized by admission")),
            address @ (MapResource::Layer { .. }
            | MapResource::Schema { .. }
            | MapResource::Style { .. }
            | MapResource::StyleRevision { .. }
            | MapResource::Feature { .. }
            | MapResource::FeatureRevision { .. }
            | MapResource::Changeset { .. }
            | MapResource::Publication { .. }
            | MapResource::Product { .. }
            | MapResource::Composition { .. }
            | MapResource::CompositionRevision { .. }) => {
                self.read_authoring_resource(address, uri, context).await
            }
        }
    }

    async fn read_dataset_resource(
        &self,
        address: MapDatasetAddress,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(context, MapScope::DatasetRead)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match address {
            MapDatasetAddress::Sources(address) => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .sources_page(&scope, &address)
                    .await
                    .map_err(internal)?,
            ),
            MapDatasetAddress::Source(address) => {
                let source = self
                    .state
                    .catalog
                    .source(&scope, address.id())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("source"))?;
                json_read(
                    uri,
                    &crate::contract::SourceSummary::new(&source).map_err(internal)?,
                )
            }
            MapDatasetAddress::MobilityProfiles(address) => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .mobility_profiles_page(&scope, &address)
                    .await
                    .map_err(internal)?,
            ),
            MapDatasetAddress::MobilityProfile(address) => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .mobility_profile(&scope, address.id(), address.version())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("mobility profile"))?,
            ),
            MapDatasetAddress::Restrictions(address) => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .restrictions_page(&scope, &address)
                    .await
                    .map_err(internal)?,
            ),
            MapDatasetAddress::Restriction(address) => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .restriction(&scope, address.id())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("restriction"))?,
            ),
            MapDatasetAddress::TravelModels(address) => json_read(
                uri,
                &crate::travel_models::TravelModelReads::new(self.state.catalog.store())
                    .page(&crate::server::tasks::runtime_owner(&identity), &address)
                    .await
                    .map_err(internal)?,
            ),
            MapDatasetAddress::TravelModel(address) => json_read(
                uri,
                &crate::travel_models::TravelModelReads::new(self.state.catalog.store())
                    .get(
                        &crate::server::tasks::runtime_owner(&identity),
                        address.id(),
                    )
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("travel model"))?,
            ),
            MapDatasetAddress::Release(address) => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .release_in_dataset(&scope, address.dataset_id(), address.release_id())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("release"))?,
            ),
            MapDatasetAddress::SourceFeature(address) => {
                self.state
                    .catalog
                    .release(&scope, address.release_id())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("dataset release"))?;
                json_read(
                    uri,
                    &self
                        .state
                        .analytics
                        .source_feature(
                            &scope.tenant_key(),
                            address.release_id(),
                            address.feature_id(),
                        )
                        .map_err(internal)?
                        .ok_or_else(|| not_found("source feature"))?,
                )
            }
            MapDatasetAddress::Raster(address) => json_read(
                uri,
                &self
                    .state
                    .analytics
                    .raster_product(&scope.tenant_key(), address.id())
                    .map_err(internal)?
                    .ok_or_else(|| not_found("raster product"))?,
            ),
            MapDatasetAddress::RasterDerivation(address) => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .raster_derivation(&scope, &identity.authority.work_context, address.id())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("raster derivation"))?,
            ),
            MapDatasetAddress::SpatialDerivation(address) => {
                require_scope(context, MapScope::SpatialDerive)?;
                json_read(
                    uri,
                    &self
                        .state
                        .catalog
                        .spatial_derivation(&scope, &identity.authority.work_context, address.id())
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("spatial derivation"))?,
                )
            }
            MapDatasetAddress::Route(address) => json_read(
                uri,
                &self
                    .state
                    .catalog
                    .route(&scope, address.id())
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("route"))?,
            ),
        }
    }
}
