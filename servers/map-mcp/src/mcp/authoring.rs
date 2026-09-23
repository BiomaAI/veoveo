use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Resource},
    service::RequestContext,
    tool_router,
};
use serde::Serialize;

use crate::{
    contract::{
        ArchiveFeatureLayerRequest, ArchiveMapCompositionRequest, BuildVectorTilesOutput,
        BuildVectorTilesRequest, CommitFeatureChangesOutput, CommitFeatureChangesRequest,
        CreateFeatureLayerRequest, CreateMapCompositionRequest, ExportFeatureLayerOutput,
        ExportFeatureLayerRequest, FeatureLayer, ImportFeatureLayerOutput,
        ImportFeatureLayerRequest, InspectGeoPackageOutput, InspectGeoPackageRequest,
        LayerPublication, MapComposition, PublishFeatureLayerRequest, QueryFeaturesOutput,
        QueryFeaturesRequest, RestoreFeatureRequest, UpdateFeatureLayerRequest,
        UpdateMapCompositionRequest, ValidateFeatureChangesOutput, ValidateFeatureChangesRequest,
    },
    uris,
};

use super::{MapMcp, internal, invalid_params, require_scope};

#[tool_router(router = authoring_tool_router, vis = "pub(super)")]
impl MapMcp {
    #[tool(
        title = "Create authored feature layer",
        description = "Create an empty feature layer owned by the current Work Context, with a JSON Schema 2020-12 contract for feature properties and an optional style.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<FeatureLayer>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn create_feature_layer(
        &self,
        Parameters(request): Parameters<CreateFeatureLayerRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:write")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let layer = self
            .state
            .authoring
            .create_layer(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        let layer_uri = uris::feature_layer_uri(layer.layer_id.as_str());
        self.state
            .subscriptions
            .notify_resource_updated(uris::FEATURE_LAYERS_URI)
            .await;
        self.state
            .subscriptions
            .notify_resource_list_changed()
            .await;
        structured_with_links(
            "created authored feature layer",
            &layer,
            [(layer_uri, "Authored feature layer")],
        )
    }

    #[tool(
        title = "Update authored feature layer",
        description = "Update a layer's metadata and add new schema or style revisions. Pass the layer revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<FeatureLayer>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn update_feature_layer(
        &self,
        Parameters(request): Parameters<UpdateFeatureLayerRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:write")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let layer = self
            .state
            .authoring
            .update_layer(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        let layer_uri = uris::feature_layer_uri(layer.layer_id.as_str());
        self.state
            .subscriptions
            .notify_resource_updated(&layer_uri)
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::FEATURE_LAYERS_URI)
            .await;
        structured_with_links(
            "updated authored feature layer",
            &layer,
            [(layer_uri, "Authored feature layer")],
        )
    }

    #[tool(
        title = "Validate authored feature changes",
        description = "Check a feature changeset without writing it: WGS84 geometry and topology, valid time, the layer's JSON Schema, and the current feature and layer revisions.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ValidateFeatureChangesOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn validate_feature_changes(
        &self,
        Parameters(request): Parameters<ValidateFeatureChangesRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:write")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .authoring
            .validate_changes(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        structured_with_links(
            format!("feature changes valid: {}", output.valid),
            &output,
            [(
                uris::feature_layer_uri(output.layer_id.as_str()),
                "Authored feature layer",
            )],
        )
    }

    #[tool(
        title = "Commit authored feature changes",
        description = "Apply a feature changeset to a layer in one step. Pass the current layer and feature revisions. Retrying with the same changeset ID has no extra effect.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommitFeatureChangesOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn commit_feature_changes(
        &self,
        Parameters(request): Parameters<CommitFeatureChangesRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:write")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .authoring
            .commit_changes(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        notify_commit(self, &context, &output).await;
        let mut links = vec![
            (
                uris::feature_layer_uri(output.changeset.layer_id.as_str()),
                "Authored feature layer",
            ),
            (
                uris::changeset_uri(
                    output.changeset.layer_id.as_str(),
                    output.changeset.changeset_id.as_str(),
                ),
                "Feature changeset",
            ),
        ];
        links.extend(output.features.iter().map(|feature| {
            (
                uris::feature_uri(feature.layer_id.as_str(), feature.id.as_str()),
                "Authored feature",
            )
        }));
        structured_with_links("committed authored feature changes", &output, links)
    }

    #[tool(
        title = "Restore authored feature",
        description = "Restore a deleted feature by adding a new live revision. Pass the current layer and feature revisions; the call fails if either has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommitFeatureChangesOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn restore_feature(
        &self,
        Parameters(request): Parameters<RestoreFeatureRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:write")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .authoring
            .restore_feature(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        notify_commit(self, &context, &output).await;
        structured_with_links(
            "restored authored feature",
            &output,
            output.features.iter().map(|feature| {
                (
                    uris::feature_uri(feature.layer_id.as_str(), feature.id.as_str()),
                    "Authored feature",
                )
            }),
        )
    }

    #[tool(
        title = "Query authored map features",
        description = "Query current or published features by WGS84 bounding box, valid-time interval, geometry type, or a CQL2 JSON filter. To get the next page, pass the `cursor` from the previous response.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<QueryFeaturesOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn query_features(
        &self,
        Parameters(request): Parameters<QueryFeaturesRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .authoring
            .query_features(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        let links = output.features.iter().map(|feature| {
            (
                uris::feature_uri(feature.layer_id.as_str(), feature.id.as_str()),
                "Authored feature",
            )
        });
        structured_with_links("queried authored map features", &output, links)
    }

    #[tool(
        title = "Publish authored feature layer",
        description = "Publish the current layer, schema, and style revisions as a fixed version that compositions and exports can use. Publishing does not add features to routing.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<LayerPublication>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn publish_feature_layer(
        &self,
        Parameters(request): Parameters<PublishFeatureLayerRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:publish")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let publication = self
            .state
            .authoring
            .publish_layer(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        let publication_uri = uris::publication_uri(
            publication.layer_id.as_str(),
            publication.publication_id.as_str(),
        );
        self.state
            .subscriptions
            .notify_resource_updated(uris::PUBLICATIONS_URI)
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(&publication_uri)
            .await;
        self.state
            .subscriptions
            .notify_resource_list_changed()
            .await;
        structured_with_links(
            "published authored feature layer",
            &publication,
            [(publication_uri, "Feature layer publication")],
        )
    }

    #[tool(
        title = "Archive authored feature layer",
        description = "Archive a feature layer. Every feature, changeset, and publication is kept. Pass the layer revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<FeatureLayer>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn archive_feature_layer(
        &self,
        Parameters(request): Parameters<ArchiveFeatureLayerRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:admin")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let layer = self
            .state
            .authoring
            .archive_layer(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        let layer_uri = uris::feature_layer_uri(layer.layer_id.as_str());
        self.state
            .subscriptions
            .notify_resource_updated(uris::FEATURE_LAYERS_URI)
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(&layer_uri)
            .await;
        self.state
            .subscriptions
            .notify_resource_list_changed()
            .await;
        structured_with_links(
            "archived authored feature layer",
            &layer,
            [(layer_uri, "Authored feature layer")],
        )
    }

    #[tool(
        title = "Create map composition",
        description = "Create a map composition: an ordered list of published feature layers, each shown with its published style.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<MapComposition>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn create_map_composition(
        &self,
        Parameters(request): Parameters<CreateMapCompositionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:write")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let composition = self
            .state
            .authoring
            .create_composition(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        notify_composition(self, &context, &composition).await;
        structured_with_links(
            "created map composition",
            &composition,
            [(
                uris::composition_uri(composition.composition_id.as_str()),
                "Map composition",
            )],
        )
    }

    #[tool(
        title = "Update map composition",
        description = "Add a new revision of a map composition. Its layers stay tied to their publications. Pass the revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<MapComposition>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn update_map_composition(
        &self,
        Parameters(request): Parameters<UpdateMapCompositionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:write")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let composition = self
            .state
            .authoring
            .update_composition(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        notify_composition(self, &context, &composition).await;
        structured_with_links(
            "updated map composition",
            &composition,
            [(
                uris::composition_revision_uri(
                    composition.composition_id.as_str(),
                    composition.current.revision,
                ),
                "Map composition revision",
            )],
        )
    }

    #[tool(
        title = "Archive map composition",
        description = "Archive a map composition. Every revision is kept. Pass the revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<MapComposition>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn archive_map_composition(
        &self,
        Parameters(request): Parameters<ArchiveMapCompositionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:feature:admin")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let composition = self
            .state
            .authoring
            .archive_composition(&identity, &scope, request)
            .await
            .map_err(invalid_params)?;
        notify_composition(self, &context, &composition).await;
        structured_with_links(
            "archived map composition",
            &composition,
            [(
                uris::composition_uri(composition.composition_id.as_str()),
                "Map composition",
            )],
        )
    }

    #[tool(
        title = "Import feature layer artifact",
        description = "Import up to 10,000 features from a GeoJSON FeatureCollection, an RFC 8142 GeoJSON text sequence, or a selected OGC GeoPackage vector table in an artifact you can read. Either every feature imports or none do. GeoPackage coordinates are converted to two-dimensional OGC:CRS84. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ImportFeatureLayerOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn import_feature_layer(
        &self,
        Parameters(_request): Parameters<ImportFeatureLayerRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`import_feature_layer` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Inspect GeoPackage artifact",
        description = "List the vector tables, fields, CRS declarations, extensions, and R-tree indexes in a GeoPackage artifact before you import it. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<InspectGeoPackageOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn inspect_geopackage(
        &self,
        Parameters(_request): Parameters<InspectGeoPackageRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`inspect_geopackage` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Export published feature layer",
        description = "Export a layer publication as an RFC 8142 GeoJSON text sequence, GeoParquet 1.0 (WKB), or an OGC GeoPackage 1.4 vector table. The result is an artifact. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ExportFeatureLayerOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn export_feature_layer(
        &self,
        Parameters(_request): Parameters<ExportFeatureLayerRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`export_feature_layer` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Build published feature vector tiles",
        description = "Build Mapbox Vector Tile 2.1 tiles and a MapLibre style from a layer publication. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<BuildVectorTilesOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn build_vector_tiles(
        &self,
        Parameters(_request): Parameters<BuildVectorTilesRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`build_vector_tiles` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }
}

async fn notify_composition(
    service: &MapMcp,
    _context: &RequestContext<RoleServer>,
    composition: &MapComposition,
) {
    service
        .state
        .subscriptions
        .notify_resource_updated(uris::COMPOSITIONS_URI)
        .await;
    service
        .state
        .subscriptions
        .notify_resource_updated(&uris::composition_uri(composition.composition_id.as_str()))
        .await;
    service
        .state
        .subscriptions
        .notify_resource_list_changed()
        .await;
}

async fn notify_commit(
    service: &MapMcp,
    _context: &RequestContext<RoleServer>,
    output: &CommitFeatureChangesOutput,
) {
    let layer_uri = uris::feature_layer_uri(output.changeset.layer_id.as_str());
    service
        .state
        .subscriptions
        .notify_resource_updated(uris::FEATURE_LAYERS_URI)
        .await;
    service
        .state
        .subscriptions
        .notify_resource_updated(&layer_uri)
        .await;
    service
        .state
        .subscriptions
        .notify_resource_updated(&uris::features_uri(output.changeset.layer_id.as_str()))
        .await;
    for feature in &output.features {
        service
            .state
            .subscriptions
            .notify_resource_updated(&uris::feature_uri(
                feature.layer_id.as_str(),
                feature.id.as_str(),
            ))
            .await;
    }
    service
        .state
        .subscriptions
        .notify_resource_list_changed()
        .await;
}

fn structured_with_links<T, I, U, L>(
    text: impl Into<String>,
    value: &T,
    links: I,
) -> Result<CallToolResult, McpError>
where
    T: Serialize,
    I: IntoIterator<Item = (U, L)>,
    U: Into<String>,
    L: Into<String>,
{
    let mut content = vec![ContentBlock::text(text)];
    content.extend(links.into_iter().map(|(uri, title)| {
        let title = title.into();
        ContentBlock::resource_link(
            Resource::new(uri.into(), title.clone())
                .with_title(title)
                .with_mime_type("application/json"),
        )
    }));
    let mut result = CallToolResult::success(content);
    result.structured_content = Some(serde_json::to_value(value).map_err(internal)?);
    Ok(result)
}
