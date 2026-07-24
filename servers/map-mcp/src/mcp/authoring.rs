use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock, Resource},
    service::RequestContext,
    tool_router,
};
use serde::Serialize;
use veoveo_mcp_contract::tool;

use crate::{
    contract::{
        ArchiveFeatureLayerRequest, ArchiveMapCompositionRequest, BuildVectorTilesOutput,
        BuildVectorTilesRequest, CommitFeatureChangesOutput, CommitFeatureChangesRequest,
        CreateFeatureLayerRequest, CreateMapCompositionRequest, ExportFeatureLayerOutput,
        ExportFeatureLayerRequest, FeatureLayer, ImportFeatureLayerOutput,
        ImportFeatureLayerRequest, LayerPublication, MapComposition, PublishFeatureLayerRequest,
        QueryFeaturesOutput, QueryFeaturesRequest, RestoreFeatureRequest,
        UpdateFeatureLayerRequest, UpdateMapCompositionRequest, ValidateFeatureChangesOutput,
        ValidateFeatureChangesRequest,
    },
    uris,
};

use super::{MapMcp, internal, invalid_params, require_scope};

#[tool_router(router = authoring_tool_router, vis = "pub(super)")]
impl MapMcp {
    #[tool(
        title = "Create authored feature layer",
        description = "Create an empty Work Context-owned feature layer with an immutable JSON Schema 2020-12 property contract and optional safe style revision.",
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
        veoveo_mcp_contract::notify_resource_list_changed(&context.peer).await;
        structured_with_links(
            "created authored feature layer",
            &layer,
            [(layer_uri, "Authored feature layer")],
        )
    }

    #[tool(
        title = "Update authored feature layer",
        description = "Replace bounded layer metadata and append immutable schema or style revisions under optimistic concurrency.",
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
        description = "Validate a bounded atomic feature changeset against WGS84 geometry, topology, valid time, JSON Schema, current feature revisions, and the current layer revision without writing it.",
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
        description = "Atomically append immutable feature revisions, update feature heads, increment the layer revision, record a scoped idempotent changeset, and publish the durable projection event.",
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
        description = "Append a new live revision for a tombstoned feature under layer and feature optimistic concurrency.",
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
        description = "Query current or published feature revisions with a WGS84 bounding box, valid-time interval, geometry type, bounded CQL2 JSON filter, and opaque keyset cursor.",
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
        description = "Create an immutable publication that pins the current layer, schema, and style revisions. Publication never promotes generic features into routing.",
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
        veoveo_mcp_contract::notify_resource_list_changed(&context.peer).await;
        structured_with_links(
            "published authored feature layer",
            &publication,
            [(publication_uri, "Feature layer publication")],
        )
    }

    #[tool(
        title = "Archive authored feature layer",
        description = "Archive a feature layer under optimistic concurrency while preserving every feature, changeset, and publication revision.",
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
        veoveo_mcp_contract::notify_resource_list_changed(&context.peer).await;
        structured_with_links(
            "archived authored feature layer",
            &layer,
            [(layer_uri, "Authored feature layer")],
        )
    }

    #[tool(
        title = "Create map composition",
        description = "Create a governed map composition whose ordered layers pin immutable feature-layer publications and their publication styles.",
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
        description = "Append an immutable composition revision with optimistic concurrency while retaining publication pins.",
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
        description = "Archive a composition under optimistic concurrency while preserving every immutable revision.",
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
        description = "Validate and atomically import up to 10000 GeoJSON FeatureCollection or RFC 8142 GeoJSON text sequence features from an authorized artifact. This operation requires durable task invocation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ImportFeatureLayerOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn import_feature_layer(
        &self,
        Parameters(_request): Parameters<ImportFeatureLayerRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "import_feature_layer requires task-based invocation",
            None,
        ))
    }

    #[tool(
        title = "Export published feature layer",
        description = "Export an immutable layer publication as RFC 8142 GeoJSON text sequence or GeoParquet 1.0 WKB through a governed artifact. This operation requires durable task invocation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ExportFeatureLayerOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn export_feature_layer(
        &self,
        Parameters(_request): Parameters<ExportFeatureLayerRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "export_feature_layer requires task-based invocation",
            None,
        ))
    }

    #[tool(
        title = "Build published feature vector tiles",
        description = "Build a bounded sorted set of Mapbox Vector Tile 2.1 tiles and a MapLibre Style projection from an immutable layer publication. This operation requires durable task invocation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<BuildVectorTilesOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn build_vector_tiles(
        &self,
        Parameters(_request): Parameters<BuildVectorTilesRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "build_vector_tiles requires task-based invocation",
            None,
        ))
    }
}

async fn notify_composition(
    service: &MapMcp,
    context: &RequestContext<RoleServer>,
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
    veoveo_mcp_contract::notify_resource_list_changed(&context.peer).await;
}

async fn notify_commit(
    service: &MapMcp,
    context: &RequestContext<RoleServer>,
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
    veoveo_mcp_contract::notify_resource_list_changed(&context.peer).await;
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
