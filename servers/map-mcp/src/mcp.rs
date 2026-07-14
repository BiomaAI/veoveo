use std::{collections::BTreeMap, sync::Arc};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock,
        GetPromptRequestParams, GetPromptResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt,
        ReadResourceRequestParams, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerInfo, SubscribeRequestParams,
        UnsubscribeRequestParams,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Serialize;
use serde_json::json;
use veoveo_mcp_contract::{GatewayInternalIdentity, Page, PlaneCaller, paginate};

use crate::{
    contract::{
        CorridorInspectionOutput, CorridorInspectionRequest, DatasetReleaseId, FacilityId,
        GeodesicDirectOutput, GeodesicDirectRequest, GeodesicInverseOutput, GeodesicInverseRequest,
        InspectLocationOutput, InspectLocationRequest, LocationId, MapDatasetId, MapSourceId,
        MobilityProfileId, PublishRestrictionRequest, ReachableArea, ReachableAreaRequest,
        RegisteredSource, RestrictionId, RestrictionMutationOutput, RouteId, RouteMatrix,
        RouteMatrixId, RouteMatrixRequest, RoutePlan, RouteRequest, RouteValidation,
        SearchLocationsOutput, SearchLocationsRequest, TransformCrsOutput, TransformCrsRequest,
        ValidateGeofenceOutput, ValidateGeofenceRequest, ValidateRouteRequest,
        WithdrawRestrictionRequest,
    },
    geodesy,
    prompts::MapPrompt,
    server::auth::ForwardedBearer,
    state::MapApplication,
    uris,
};

const LIST_PAGE_SIZE: usize = 100;

#[derive(Clone)]
pub struct MapMcp {
    state: Arc<MapApplication>,
    #[allow(dead_code)]
    tool_router: ToolRouter<MapMcp>,
}

#[tool_router]
impl MapMcp {
    pub fn new(state: Arc<MapApplication>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Search map locations",
        description = "Find authorized named locations and facilities inside an explicit WGS84 bounding box.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SearchLocationsOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn search_locations(
        &self,
        Parameters(request): Parameters<SearchLocationsRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:dataset:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .analytics
            .search_locations(&scope.tenant_key(), &request)
            .map_err(invalid_params)?;
        structured_result(
            format!("found {} location(s)", output.locations.len()),
            &output,
        )
    }

    #[tool(
        title = "Inspect map location",
        description = "Describe one named location, nearby facilities, containing boundaries, source lineage, and explicit data gaps.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<InspectLocationOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn inspect_location(
        &self,
        Parameters(request): Parameters<InspectLocationRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:dataset:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .geography
            .inspect_location(&scope, request)
            .map_err(invalid_params)?;
        structured_result(
            format!("inspected {}", output.location.location_id),
            &output,
        )
    }

    #[tool(
        title = "Transform coordinate reference system",
        description = "Transform bounded two-dimensional coordinates between explicit CRS ids through PROJ. Vertical coordinates are rejected rather than copied.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TransformCrsOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn transform_crs(
        &self,
        Parameters(request): Parameters<TransformCrsRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, "map:dataset:read")?;
        let output = geodesy::transform_crs(request).map_err(invalid_params)?;
        structured_result(
            format!("transformed {} position(s)", output.positions.len()),
            &output,
        )
    }

    #[tool(
        title = "Calculate inverse geodesic",
        description = "Calculate WGS84 ellipsoidal distance and forward and reverse azimuths between two positions.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<GeodesicInverseOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn geodesic_inverse(
        &self,
        Parameters(request): Parameters<GeodesicInverseRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, "map:dataset:read")?;
        let output = geodesy::geodesic_inverse(request).map_err(invalid_params)?;
        structured_result(format!("distance {:.3} m", output.distance.get()), &output)
    }

    #[tool(
        title = "Calculate direct geodesic",
        description = "Calculate a WGS84 destination from a start position, azimuth, and ellipsoidal distance.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<GeodesicDirectOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn geodesic_direct(
        &self,
        Parameters(request): Parameters<GeodesicDirectRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, "map:dataset:read")?;
        let output = geodesy::geodesic_direct(request).map_err(invalid_params)?;
        structured_result("calculated geodesic destination".to_owned(), &output)
    }

    #[tool(
        title = "Validate geographic geofence",
        description = "Validate a WGS84 path against a topologically valid WGS84 geofence and an explicit containment rule.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ValidateGeofenceOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn validate_geofence(
        &self,
        Parameters(request): Parameters<ValidateGeofenceRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, "map:dataset:read")?;
        let output = geodesy::validate_geofence(request).map_err(invalid_params)?;
        structured_result(format!("geofence valid: {}", output.valid), &output)
    }

    #[tool(
        title = "Calculate logistics route",
        description = "Calculate a governed route for one versioned human or vehicle mobility profile. The result pins releases, restrictions, a snapshot, costs, and validation state; unavailable coverage is never replaced by a straight line.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RoutePlan>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn route(
        &self,
        Parameters(request): Parameters<RouteRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:route")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .routes
            .route(&scope, request)
            .await
            .map_err(invalid_params)?;
        let _ = context.peer.notify_resource_list_changed().await;
        structured_result(format!("planned route {}", output.route_id), &output)
    }

    #[tool(
        title = "Calculate logistics route matrix",
        description = "Calculate a bounded many-to-many route matrix for one versioned mobility profile. Task-capable clients should invoke this as a durable task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RouteMatrix>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn route_matrix(
        &self,
        Parameters(_request): Parameters<RouteMatrixRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "route_matrix requires task-based invocation",
            None,
        ))
    }

    #[tool(
        title = "Calculate land reachable area",
        description = "Calculate a governed Valhalla network isochrone for a human or road-vehicle profile. This operation requires durable task invocation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ReachableArea>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn reachable_area(
        &self,
        Parameters(_request): Parameters<ReachableAreaRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "reachable_area requires task-based invocation",
            None,
        ))
    }

    #[tool(
        title = "Validate logistics route",
        description = "Validate supplied route geometry, pinned release availability, profile availability, and active prohibitions.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RouteValidation>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn validate_route(
        &self,
        Parameters(request): Parameters<ValidateRouteRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:route")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .routes
            .validate_route(&scope, request)
            .await
            .map_err(invalid_params)?;
        structured_result(format!("route valid: {}", output.valid), &output)
    }

    #[tool(
        title = "Inspect logistics corridor",
        description = "Inspect a WGS84 corridor for effective restrictions, facilities, boundaries, and explicit data gaps.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CorridorInspectionOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn inspect_corridor(
        &self,
        Parameters(request): Parameters<CorridorInspectionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:dataset:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .geography
            .inspect_corridor(&scope, request)
            .await
            .map_err(invalid_params)?;
        structured_result(
            format!(
                "found {} effective restriction(s)",
                output.restrictions.len()
            ),
            &output,
        )
    }

    #[tool(
        title = "Publish operational restriction",
        description = "Publish a governed, effective, versioned transport restriction. Authority and validity are explicit.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RestrictionMutationOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn publish_restriction(
        &self,
        Parameters(request): Parameters<PublishRestrictionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:restriction:publish")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let restriction = self
            .state
            .geography
            .publish_restriction(&scope, request)
            .await
            .map_err(invalid_params)?;
        let output = RestrictionMutationOutput {
            restriction,
            invalidated_route_count: 0,
        };
        self.state
            .subscriptions
            .notify_resource_updated(uris::RESTRICTIONS_URI)
            .await;
        let _ = context.peer.notify_resource_list_changed().await;
        structured_result("published restriction".to_owned(), &output)
    }

    #[tool(
        title = "Withdraw operational restriction",
        description = "End an existing restriction under optimistic concurrency and record its cancellation identity.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RestrictionMutationOutput>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn withdraw_restriction(
        &self,
        Parameters(request): Parameters<WithdrawRestrictionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "map:restriction:withdraw")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let (restriction, invalidated_route_count) = self
            .state
            .geography
            .withdraw_restriction(&scope, request)
            .await
            .map_err(invalid_params)?;
        let output = RestrictionMutationOutput {
            restriction,
            invalidated_route_count,
        };
        self.state
            .subscriptions
            .notify_resource_updated(uris::RESTRICTIONS_URI)
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::ROUTES_URI)
            .await;
        structured_result("withdrew restriction".to_owned(), &output)
    }
}

#[tool_handler]
impl ServerHandler for MapMcp {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new("map", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Earth geography and logistics planning for human, road, off-road, rail, maritime, and aviation mobility. Read versioned map:// resources, use route or route_matrix with an explicit profile and departure time, and treat planning_advisory status as non-certified guidance. The server never fabricates a straight-line route when transport coverage is unavailable."
                .to_owned(),
        );
        info
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let page = mcp_page(tools, request.as_ref())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let identity = require_scope(&context, "map:dataset:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let mut resources = root_resources();
        for source in self
            .state
            .catalog
            .list_sources(&scope)
            .await
            .map_err(internal)?
        {
            resources.push(json_resource_descriptor(
                uris::source_uri(source.source_id.as_str()),
                format!("source {}", source.name),
                "Authorized source provenance without acquisition secrets.",
            ));
        }
        for release in self
            .state
            .catalog
            .list_releases(&scope)
            .await
            .map_err(internal)?
        {
            resources.push(json_resource_descriptor(
                uris::release_uri(release.dataset_id.as_str(), release.release_id.as_str()),
                format!("release {}", release.version_label),
                "Immutable governed dataset release.",
            ));
        }
        for location in self
            .state
            .analytics
            .list_locations(&scope.tenant_key(), 10_000)
            .map_err(internal)?
        {
            resources.push(json_resource_descriptor(
                uris::location_uri(location.location_id.as_str()),
                location.name,
                "Named Earth location with source lineage.",
            ));
        }
        for facility in self
            .state
            .analytics
            .list_facilities(&scope.tenant_key(), 10_000)
            .map_err(internal)?
        {
            resources.push(json_resource_descriptor(
                uris::facility_uri(facility.facility_id.as_str()),
                facility.name,
                "Logistics facility and transfer point.",
            ));
        }
        for profile in self
            .state
            .catalog
            .list_mobility_profiles(&scope)
            .await
            .map_err(internal)?
        {
            let metadata = profile.metadata();
            resources.push(json_resource_descriptor(
                uris::mobility_profile_uri(metadata.profile_id.as_str(), metadata.version),
                metadata.name.clone(),
                "Versioned human or vehicle mobility profile.",
            ));
        }
        for restriction in self
            .state
            .catalog
            .list_restrictions(&scope)
            .await
            .map_err(internal)?
        {
            resources.push(json_resource_descriptor(
                uris::restriction_uri(restriction.restriction_id.as_str()),
                format!("restriction {}", restriction.restriction_id),
                "Effective governed transport restriction.",
            ));
        }
        for route in self
            .state
            .catalog
            .list_routes(&scope)
            .await
            .map_err(internal)?
        {
            resources.push(json_resource_descriptor(
                uris::route_uri(route.route_id.as_str()),
                format!("route {}", route.route_id),
                "Owner-scoped route with pinned provenance.",
            ));
        }
        for matrix in self
            .state
            .catalog
            .list_matrices(&scope)
            .await
            .map_err(internal)?
        {
            resources.push(json_resource_descriptor(
                uris::matrix_uri(matrix.matrix_id.as_str()),
                format!("matrix {}", matrix.matrix_id),
                "Owner-scoped many-to-many route matrix.",
            ));
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        let page = mcp_page(resources, request.as_ref())?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let templates = vec![
            template(
                uris::SOURCE_TEMPLATE,
                "Map source",
                "Authorized source provenance.",
            ),
            template(
                uris::DATASET_TEMPLATE,
                "Map dataset",
                "Release index for one dataset.",
            ),
            template(
                uris::RELEASE_TEMPLATE,
                "Dataset release",
                "Immutable governed release.",
            ),
            template(
                uris::LOCATION_TEMPLATE,
                "Map location",
                "Named location with lineage.",
            ),
            template(
                uris::FACILITY_TEMPLATE,
                "Map facility",
                "Logistics facility.",
            ),
            template(
                uris::MOBILITY_PROFILE_TEMPLATE,
                "Mobility profile",
                "Versioned mobility constraints.",
            ),
            template(
                uris::RESTRICTION_TEMPLATE,
                "Map restriction",
                "Effective restriction.",
            ),
            template(uris::ROUTE_TEMPLATE, "Map route", "Owner-scoped route."),
            template(
                uris::MATRIX_TEMPLATE,
                "Route matrix",
                "Owner-scoped route matrix.",
            ),
            template(
                uris::ARTIFACT_TEMPLATE,
                "Map artifact",
                "Governed immutable map artifact.",
            ),
        ];
        let page = mcp_page(templates, request.as_ref())?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(&context, "map:dataset:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let uri = request.uri.as_str();
        match uri {
            uris::SOURCES_URI => {
                let sources = self
                    .state
                    .catalog
                    .list_sources(&scope)
                    .await
                    .map_err(internal)?;
                return json_resource(uri, &sources.iter().map(public_source).collect::<Vec<_>>());
            }
            uris::DATASETS_URI => {
                return json_resource(
                    uri,
                    &dataset_index(
                        self.state
                            .catalog
                            .list_releases(&scope)
                            .await
                            .map_err(internal)?,
                    ),
                );
            }
            uris::LOCATIONS_URI => {
                return json_resource(
                    uri,
                    &self
                        .state
                        .analytics
                        .list_locations(&scope.tenant_key(), 10_000)
                        .map_err(internal)?,
                );
            }
            uris::FACILITIES_URI => {
                return json_resource(
                    uri,
                    &self
                        .state
                        .analytics
                        .list_facilities(&scope.tenant_key(), 10_000)
                        .map_err(internal)?,
                );
            }
            uris::MOBILITY_PROFILES_URI => {
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .list_mobility_profiles(&scope)
                        .await
                        .map_err(internal)?,
                );
            }
            uris::RESTRICTIONS_URI => {
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .list_restrictions(&scope)
                        .await
                        .map_err(internal)?,
                );
            }
            uris::ROUTES_URI => {
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .list_routes(&scope)
                        .await
                        .map_err(internal)?,
                );
            }
            uris::MATRICES_URI => {
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .list_matrices(&scope)
                        .await
                        .map_err(internal)?,
                );
            }
            _ => {}
        }
        if let Some(value) = uris::parse_single(uri, "map://source/") {
            let id = MapSourceId::parse(value).map_err(invalid_params)?;
            let source = self
                .state
                .catalog
                .source(&scope, &id)
                .await
                .map_err(internal)?
                .ok_or_else(|| not_found("source"))?;
            return json_resource(uri, &public_source(&source));
        }
        if let Some((dataset, release)) = uris::parse_release(uri) {
            let dataset_id = MapDatasetId::parse(dataset).map_err(invalid_params)?;
            let release_id = DatasetReleaseId::parse(release).map_err(invalid_params)?;
            let release = self
                .state
                .catalog
                .release(&scope, &release_id)
                .await
                .map_err(internal)?
                .filter(|release| release.dataset_id == dataset_id)
                .ok_or_else(|| not_found("release"))?;
            return json_resource(uri, &release);
        }
        if let Some(value) = uris::parse_single(uri, "map://dataset/") {
            let id = MapDatasetId::parse(value).map_err(invalid_params)?;
            let releases = self
                .state
                .catalog
                .list_releases(&scope)
                .await
                .map_err(internal)?
                .into_iter()
                .filter(|release| release.dataset_id == id)
                .collect::<Vec<_>>();
            if releases.is_empty() {
                return Err(not_found("dataset"));
            }
            return json_resource(uri, &releases);
        }
        if let Some(value) = uris::parse_single(uri, "map://location/") {
            let id = LocationId::parse(value).map_err(invalid_params)?;
            return json_resource(
                uri,
                &self
                    .state
                    .analytics
                    .location(&scope.tenant_key(), &id)
                    .map_err(internal)?
                    .ok_or_else(|| not_found("location"))?,
            );
        }
        if let Some(value) = uris::parse_single(uri, "map://facility/") {
            let id = FacilityId::parse(value).map_err(invalid_params)?;
            return json_resource(
                uri,
                &self
                    .state
                    .analytics
                    .facility(&scope.tenant_key(), &id)
                    .map_err(internal)?
                    .ok_or_else(|| not_found("facility"))?,
            );
        }
        if let Some((value, version)) = uris::parse_profile(uri) {
            let id = MobilityProfileId::parse(value).map_err(invalid_params)?;
            return json_resource(
                uri,
                &self
                    .state
                    .catalog
                    .mobility_profile(&scope, &id, version)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("mobility profile"))?,
            );
        }
        if let Some(value) = uris::parse_single(uri, "map://restriction/") {
            let id = RestrictionId::parse(value).map_err(invalid_params)?;
            return json_resource(
                uri,
                &self
                    .state
                    .catalog
                    .restriction(&scope, &id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("restriction"))?,
            );
        }
        if let Some(value) = uris::parse_single(uri, "map://route/") {
            let id = RouteId::parse(value).map_err(invalid_params)?;
            return json_resource(
                uri,
                &self
                    .state
                    .catalog
                    .route(&scope, &id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("route"))?,
            );
        }
        if let Some(value) = uris::parse_single(uri, "map://matrix/") {
            let id = RouteMatrixId::parse(value).map_err(invalid_params)?;
            return json_resource(
                uri,
                &self
                    .state
                    .catalog
                    .matrix(&scope, &id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("matrix"))?,
            );
        }
        if let Some(artifact_id) = uris::parse_artifact(uri) {
            let artifact = self
                .state
                .artifacts
                .get(&internal_caller(&context)?, &artifact_id)
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
            return Ok(ReadResourceResult::new(vec![content]));
        }
        Err(McpError::resource_not_found(
            format!("unknown Map resource `{uri}`"),
            None,
        ))
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts: Vec<Prompt> = MapPrompt::ALL
            .into_iter()
            .map(MapPrompt::definition)
            .collect();
        let page = mcp_page(prompts, request.as_ref())?;
        Ok(ListPromptsResult {
            prompts: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        MapPrompt::by_name(&request.name)
            .ok_or_else(|| McpError::invalid_params("unknown Map prompt", None))?
            .render(request.arguments)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let identity = require_scope(&context, "map:dataset:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let values = completion_values(&self.state, &scope, &reference.uri, &request.argument.name)
            .await
            .map_err(internal)?;
        let needle = request.argument.value.to_ascii_lowercase();
        let matching = values
            .into_iter()
            .filter(|value| value.to_ascii_lowercase().contains(&needle))
            .collect::<Vec<_>>();
        let total = matching.len();
        let values = matching
            .into_iter()
            .take(CompletionInfo::MAX_VALUES)
            .collect::<Vec<_>>();
        Ok(CompleteResult::new(
            CompletionInfo::with_pagination(
                values,
                Some(total as u32),
                total > CompletionInfo::MAX_VALUES,
            )
            .map_err(internal)?,
        ))
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = require_scope(&context, "map:dataset:read")?;
        if !is_subscribable(&request.uri) {
            return Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ));
        }
        self.state
            .subscriptions
            .subscribe(request.uri, identity.principal.id, context.peer.clone())
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = require_scope(&context, "map:dataset:read")?;
        if !is_subscribable(&request.uri) {
            return Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ));
        }
        self.state
            .subscriptions
            .unsubscribe(&request.uri, &identity.principal.id)
            .await;
        Ok(())
    }
}

fn internal_identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, McpError> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<GatewayInternalIdentity>())
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))
}

fn internal_caller(context: &RequestContext<RoleServer>) -> Result<PlaneCaller, McpError> {
    let identity = internal_identity(context)?;
    let bearer_token = context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<ForwardedBearer>())
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    let memberships = identity.principal.group_memberships();
    Ok(PlaneCaller {
        identity,
        memberships,
        bearer_token,
    })
}

fn require_scope(
    context: &RequestContext<RoleServer>,
    required: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    if !identity
        .principal
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
    {
        return Err(McpError::invalid_request(
            format!("scope `{required}` is required"),
            None,
        ));
    }
    Ok(identity)
}

fn structured_result<T: Serialize>(text: String, value: &T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(value).map_err(internal)?);
    Ok(result)
}

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, McpError> {
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(serde_json::to_string(value).map_err(internal)?, uri)
            .with_mime_type("application/json"),
    ]))
}

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE).map_err(invalid_params)
}

fn invalid_params(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

fn not_found(kind: &str) -> McpError {
    McpError::resource_not_found(format!("unknown {kind}"), None)
}

fn root_resources() -> Vec<Resource> {
    [
        (uris::DATASETS_URI, "Map datasets"),
        (uris::SOURCES_URI, "Map sources"),
        (uris::LOCATIONS_URI, "Map locations"),
        (uris::FACILITIES_URI, "Map facilities"),
        (uris::MOBILITY_PROFILES_URI, "Mobility profiles"),
        (uris::RESTRICTIONS_URI, "Map restrictions"),
        (uris::ROUTES_URI, "Map routes"),
        (uris::MATRICES_URI, "Route matrices"),
    ]
    .into_iter()
    .map(|(uri, title)| {
        json_resource_descriptor(
            uri.to_owned(),
            title.to_owned(),
            "Authorized Map domain index.",
        )
    })
    .collect()
}

fn json_resource_descriptor(uri: String, title: String, description: &str) -> Resource {
    Resource::new(uri, title.clone())
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}

fn template(uri: &str, title: &str, description: &str) -> ResourceTemplate {
    ResourceTemplate::new(uri, title)
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}

fn public_source(source: &RegisteredSource) -> serde_json::Value {
    json!({
        "source_id": source.source_id,
        "dataset_id": source.dataset_id,
        "name": source.name,
        "adapter_kind": source.adapter_kind,
        "authority": source.authority,
        "acquisition_model": source.acquisition_model,
        "map_families": source.map_families,
        "license": source.license,
        "enabled": source.enabled,
        "record_version": source.record_version,
        "created_at": source.created_at,
        "updated_at": source.updated_at,
    })
}

fn dataset_index(
    releases: Vec<crate::contract::DatasetRelease>,
) -> BTreeMap<String, Vec<crate::contract::DatasetRelease>> {
    let mut index = BTreeMap::new();
    for release in releases {
        index
            .entry(release.dataset_id.to_string())
            .or_insert_with(Vec::new)
            .push(release);
    }
    index
}

async fn completion_values(
    state: &MapApplication,
    scope: &crate::catalog::MapScope,
    template: &str,
    argument: &str,
) -> anyhow::Result<Vec<String>> {
    let values = match (template, argument) {
        (uris::SOURCE_TEMPLATE, "source_id") => state
            .catalog
            .list_sources(scope)
            .await?
            .into_iter()
            .map(|value| value.source_id.to_string())
            .collect(),
        (uris::DATASET_TEMPLATE | uris::RELEASE_TEMPLATE, "dataset_id") => state
            .catalog
            .list_releases(scope)
            .await?
            .into_iter()
            .map(|value| value.dataset_id.to_string())
            .collect(),
        (uris::RELEASE_TEMPLATE, "release_id") => state
            .catalog
            .list_releases(scope)
            .await?
            .into_iter()
            .map(|value| value.release_id.to_string())
            .collect(),
        (uris::LOCATION_TEMPLATE, "location_id") => state
            .analytics
            .list_locations(&scope.tenant_key(), 10_000)?
            .into_iter()
            .map(|value| value.location_id.to_string())
            .collect(),
        (uris::FACILITY_TEMPLATE, "facility_id") => state
            .analytics
            .list_facilities(&scope.tenant_key(), 10_000)?
            .into_iter()
            .map(|value| value.facility_id.to_string())
            .collect(),
        (uris::MOBILITY_PROFILE_TEMPLATE, "profile_id") => state
            .catalog
            .list_mobility_profiles(scope)
            .await?
            .into_iter()
            .map(|value| value.metadata().profile_id.to_string())
            .collect(),
        (uris::MOBILITY_PROFILE_TEMPLATE, "profile_version") => state
            .catalog
            .list_mobility_profiles(scope)
            .await?
            .into_iter()
            .map(|value| value.metadata().version.to_string())
            .collect(),
        (uris::RESTRICTION_TEMPLATE, "restriction_id") => state
            .catalog
            .list_restrictions(scope)
            .await?
            .into_iter()
            .map(|value| value.restriction_id.to_string())
            .collect(),
        (uris::ROUTE_TEMPLATE, "route_id") => state
            .catalog
            .list_routes(scope)
            .await?
            .into_iter()
            .map(|value| value.route_id.to_string())
            .collect(),
        (uris::MATRIX_TEMPLATE, "matrix_id") => state
            .catalog
            .list_matrices(scope)
            .await?
            .into_iter()
            .map(|value| value.matrix_id.to_string())
            .collect(),
        _ => Vec::new(),
    };
    let mut values = values;
    values.sort();
    values.dedup();
    Ok(values)
}

fn is_subscribable(uri: &str) -> bool {
    matches!(
        uri,
        uris::DATASETS_URI
            | uris::MOBILITY_PROFILES_URI
            | uris::RESTRICTIONS_URI
            | uris::ROUTES_URI
    ) || uris::parse_profile(uri).is_some()
        || uris::parse_single(uri, "map://restriction/").is_some()
        || uris::parse_single(uri, "map://route/").is_some()
        || uris::parse_single(uri, "map://dataset/").is_some()
}
