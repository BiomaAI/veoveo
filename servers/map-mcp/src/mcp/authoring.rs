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
        ArchiveFeatureLayerRequest, CommitFeatureChangesOutput, CommitFeatureChangesRequest,
        CreateFeatureLayerRequest, FeatureLayer, LayerPublication, PublishFeatureLayerRequest,
        QueryFeaturesOutput, QueryFeaturesRequest, RestoreFeatureRequest,
        UpdateFeatureLayerRequest, ValidateFeatureChangesOutput, ValidateFeatureChangesRequest,
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
        let _ = context.peer.notify_resource_list_changed().await;
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
        let _ = context.peer.notify_resource_list_changed().await;
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
        let _ = context.peer.notify_resource_list_changed().await;
        structured_with_links(
            "archived authored feature layer",
            &layer,
            [(layer_uri, "Authored feature layer")],
        )
    }
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
    let _ = context.peer.notify_resource_list_changed().await;
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
