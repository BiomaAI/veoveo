use std::sync::{Arc, LazyLock};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo,
        GetPromptRequestParams, GetPromptResult, Prompt, ReadResourceRequestParams,
        ReadResourceResult, Reference, Resource, ResourceContents, ResourceTemplate, Tool,
    },
    service::RequestContext,
    tool_router,
};
use veoveo_mcp_contract::{
    GatewayInternalIdentity, SubscriptionHub,
    docs::ServerDocs,
    hosting::{
        DomainAddress, DomainRead, DomainServer, Listing, ResourceSubscriptions, gateway_identity,
        json_read, plane_caller, served_by_host, structured_result, unknown_prompt,
    },
    server_contract::McpServerSetup,
};
use veoveo_types::ScopeDefinition;

use crate::{
    administration::{self, AdminOpError},
    contract::MapAddress,
    contract::{
        AcquisitionJob, BuildTravelModelRequest, CancelAcquisitionRequest,
        CorridorInspectionOutput, CorridorInspectionRequest, CreateAcquisitionRequest,
        CreateMobilityProfileRequest, CreateSourceRequest, DeriveRasterRequest,
        DeriveSpatialGeometryRequest, DisableSourceRequest, GeodesicDirectOutput,
        GeodesicDirectRequest, GeodesicInverseOutput, GeodesicInverseRequest,
        InspectLocationOutput, InspectLocationRequest, InspectPositionOutput,
        InspectPositionRequest, ListActiveDatasetReleasesOutput, ListActiveDatasetReleasesRequest,
        MapDatasetId, MapRouteHandoff, MapScope, MobilityProfile, MobilityProfileId,
        PrepareRouteHandoffRequest, PublishRestrictionRequest, QuerySourceFeaturesOutput,
        QuerySourceFeaturesRequest, RasterDerivation, ReachableArea, ReachableAreaRequest,
        RegisteredSource, ReleaseMutationRequest, ReleaseMutationResponse, ReplaceSourceRequest,
        RestrictionMutationOutput, RouteMatrix, RouteMatrixRequest, RoutePlan, RouteRequest,
        RouteValidation, SearchLocationsRequest, SpatialDerivation, TransformCrsOutput,
        TransformCrsRequest, TravelModelRecord, ValidateGeofenceOutput, ValidateGeofenceRequest,
        ValidateRouteRequest, WithdrawRestrictionRequest,
    },
    geodesy,
    prompts::MapPrompt,
    state::MapApplication,
    uris,
};

mod authoring;
mod completion;
mod derivations;
mod discovery;
mod resources;
pub(crate) mod setup;
#[cfg(test)]
use discovery::resource_templates;
#[cfg(test)]
use discovery::stable_resource_uris;
use discovery::{ResourceDiscoveryAccess, discoverable_resources};
use setup::MapContract;
mod knowledge;
mod metadata;
mod owned;

/// The crate documents embedded at build time and served under the well-known
/// surface: `map://docs`, `map://docs/{doc_id}`, `map://contract`, and the
/// administrative `admin/docs` routes (contract C18-C21).
pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> = LazyLock::new(|| {
    veoveo_mcp_contract::server_docs!("map")
        .with_embedded_doc(
            "resources",
            "Map resources and contracts",
            veoveo_mcp_contract::docs::embedded_document!("RESOURCES.md"),
        )
        .with_embedded_doc(
            "routing",
            "Map routing",
            veoveo_mcp_contract::docs::embedded_document!("ROUTING.md"),
        )
        .with_embedded_doc(
            "authoring",
            "Map feature authoring",
            veoveo_mcp_contract::docs::embedded_document!("AUTHORING.md"),
        )
        .with_embedded_doc(
            "acquisition",
            "Map source acquisition",
            veoveo_mcp_contract::docs::embedded_document!("ACQUISITION.md"),
        )
});

/// Scopes that may read the well-known surface; the same set gates
/// `list_resources`, so any identity able to list resources can read the
/// server's manual and contract declaration.
const WELL_KNOWN_SCOPES: &[MapScope] = &[
    MapScope::DatasetRead,
    MapScope::FeatureRead,
    MapScope::Admin,
];

/// Tools the Map workspace may invoke. Each remains scope-gated in its
/// handler; the workspace access resource only controls presentation.
const WORKSPACE_TOOLS: &[&str] = &[
    "register_source",
    "replace_source",
    "disable_source",
    "start_acquisition",
    "cancel_acquisition",
    "register_mobility_profile",
    "activate_release",
    "rollback_release",
    "quarantine_release",
    "archive_feature_layer",
    "archive_map_composition",
    "build_vector_tiles",
    "commit_feature_changes",
    "create_feature_layer",
    "create_map_composition",
    "export_feature_layer",
    "import_feature_layer",
    "inspect_geopackage",
    "publish_feature_layer",
    "query_features",
    "query_source_features",
    "restore_feature",
    "update_feature_layer",
    "update_map_composition",
    "validate_feature_changes",
];

/// Self-contained icon for the workspace (lucide `map-pinned` outline).
const WORKSPACE_APP_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNCIgaGVpZ2h0PSIyNCIgdmlld0JveD0iMCAwIDI0IDI0IiBmaWxsPSJub25lIiBzdHJva2U9IiM0YTdkZDYiIHN0cm9rZS13aWR0aD0iMiIgc3Ryb2tlLWxpbmVjYXA9InJvdW5kIiBzdHJva2UtbGluZWpvaW49InJvdW5kIj48cGF0aCBkPSJNMTggOGMwIDMuNjEzLTMuODY5IDcuNDI5LTUuMzkzIDguNzk1YTEgMSAwIDAgMS0xLjIxNCAwQzkuODcgMTUuNDI5IDYgMTEuNjEzIDYgOGE2IDYgMCAwIDEgMTIgMCIvPjxjaXJjbGUgY3g9IjEyIiBjeT0iOCIgcj0iMiIvPjxwYXRoIGQ9Ik04LjcxNCAxNGgtMy43MWExIDEgMCAwIDAtLjk0OC42ODNsLTIuMDA0IDZBMSAxIDAgMCAwIDMgMjJoMThhMSAxIDAgMCAwIC45NDgtMS4zMTZsLTItNmExIDEgMCAwIDAtLjk0OS0uNjg0aC0zLjcxMiIvPjwvc3ZnPg==";

#[derive(Clone)]
pub struct MapMcp {
    workspace_app: veoveo_mcp_apps_extension::AppHtml,
    state: Arc<MapApplication>,
    tool_router: ToolRouter<MapMcp>,
}

#[tool_router]
impl MapMcp {
    pub fn new(
        state: Arc<MapApplication>,
        workspace_app: veoveo_mcp_apps_extension::AppHtml,
    ) -> Self {
        Self {
            workspace_app,
            state,
            tool_router: Self::full_tool_router(),
        }
    }

    /// Every registered Map tool: the base router merged with the authoring
    /// router. Both the served handler and the `map://contract` capability
    /// inventory build from this one registration.
    fn full_tool_router() -> ToolRouter<MapMcp> {
        let mut tool_router = Self::tool_router();
        tool_router.merge(Self::authoring_tool_router());
        tool_router
    }

    /// The capability inventory declared at `map://contract` (contract C19).
    ///
    #[tool(
        title = "Search map locations",
        description = "Find authorized named locations and facilities inside an explicit WGS84 bounding box.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_mcp_knowledge_extension::SearchResults>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn search_locations(
        &self,
        Parameters(request): Parameters<SearchLocationsRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, MapScope::DatasetRead)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .analytics
            .search_locations(&scope.tenant_key(), &request)
            .map_err(invalid_params)?;
        Ok(veoveo_mcp_knowledge_extension::server::search_result(
            output,
        ))
    }

    #[tool(
        title = "List active dataset releases",
        description = "List Map dataset releases and show which one is active, optionally for one source or dataset.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ListActiveDatasetReleasesOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn list_active_dataset_releases(
        &self,
        Parameters(request): Parameters<ListActiveDatasetReleasesRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if !(1..=100).contains(&request.limit) {
            return Err(invalid_params("limit must be within 1..=100"));
        }
        let identity = require_scope(&context, MapScope::DatasetRead)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .catalog
            .active_releases(&scope, &request)
            .await
            .map_err(internal)?;
        structured_result(
            format!(
                "resolved {} active dataset release(s)",
                output.releases.len()
            ),
            &output,
        )
    }

    #[tool(
        title = "Query immutable source features",
        description = "Query point, line, polygon, and relation features in one Map release. Filter by source, exact or present tags, text, and spatial predicates; distances are in WGS84 meters. Results come in a stable order, by feature or by distance. To get the next page, pass the `cursor` from the previous response.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<QuerySourceFeaturesOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn query_source_features(
        &self,
        Parameters(request): Parameters<QuerySourceFeaturesRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, MapScope::DatasetRead)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        request.validate().map_err(invalid_params)?;
        let release = self
            .state
            .catalog
            .release(&scope, &request.release_id)
            .await
            .map_err(internal)?
            .ok_or_else(|| not_found("dataset release"))?;
        if request
            .source_id
            .as_ref()
            .is_some_and(|source_id| *source_id != release.source_id)
        {
            return Err(invalid_params(
                "source_id does not own the selected immutable release",
            ));
        }
        let output = self
            .state
            .analytics
            .query_source_features(&scope.tenant_key(), &request)
            .map_err(invalid_params)?;
        structured_result(
            format!(
                "matched {} source feature(s) in {}",
                output.features.len(),
                output.release_id
            ),
            &output,
        )
    }

    #[tool(
        title = "Derive a governed raster product",
        description = "Run one operation on a Map raster product: sample, terrain-corridor maximum, window, class mask, contour, polygonize, skeletonize, or line derivation. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RasterDerivation>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn derive_raster(
        &self,
        Parameters(_request): Parameters<DeriveRasterRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`derive_raster` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Derive governed spatial geometry",
        description = "Derive mission geometry for one mobility profile: resampling, tours, boundaries, standoffs, corridors, parallel lanes, racetracks, stations, coverage tracks, connected components, ingress geometry, or a full-route check. The result records the source releases, restrictions, terrain classes, and algorithm revision it used.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SpatialDerivation>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn derive_spatial_geometry(
        &self,
        Parameters(request): Parameters<DeriveSpatialGeometryRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, MapScope::SpatialDerive)?;
        crate::server::auth::require_scope(&identity.actor.scopes, MapScope::DatasetRead)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let derivation = self
            .state
            .spatial
            .derive(&scope, &identity, request)
            .await
            .map_err(invalid_params)?;
        self.state
            .subscriptions
            .notify_resource_updated(uris::SPATIAL_DERIVATIONS_URI)
            .await;
        self.state
            .subscriptions
            .notify_resource_contents_changed()
            .await;
        structured_result(
            format!(
                "derived {} spatial geometry product(s); valid: {}",
                derivation.geometries.len(),
                derivation.valid
            ),
            &derivation,
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
        let identity = require_scope(&context, MapScope::DatasetRead)?;
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
        title = "Inspect map position",
        description = "Look up what is at a WGS84 position in the active Map releases: nearby named locations and facilities by distance, containing boundaries, the releases used, and any coverage gaps. By default it searches a 10 km radius and returns five results per entity class.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<InspectPositionOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn inspect_position(
        &self,
        Parameters(request): Parameters<InspectPositionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, MapScope::DatasetRead)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .geography
            .inspect_position(&scope, request)
            .map_err(invalid_params)?;
        structured_result(
            format!(
                "inspected position; found {} nearby location(s) and {} nearby facility/facilities",
                output.nearby_locations.len(),
                output.nearby_facilities.len()
            ),
            &output,
        )
    }

    #[tool(
        title = "Transform coordinate reference system",
        description = "Convert two-dimensional coordinates between CRS ids with PROJ. Inputs with a vertical coordinate are rejected.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TransformCrsOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn transform_crs(
        &self,
        Parameters(request): Parameters<TransformCrsRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, MapScope::DatasetRead)?;
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
        require_scope(&context, MapScope::DatasetRead)?;
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
        require_scope(&context, MapScope::DatasetRead)?;
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
        require_scope(&context, MapScope::DatasetRead)?;
        let output = geodesy::validate_geofence(request).map_err(invalid_params)?;
        structured_result(format!("geofence valid: {}", output.valid), &output)
    }

    #[tool(
        title = "Calculate logistics route",
        description = "Calculate a route for one human or vehicle mobility profile. The result records the releases, restrictions, and snapshot it used, with costs and validation state. Where map coverage is missing, the route fails instead of drawing a straight line. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RoutePlan>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn route(
        &self,
        Parameters(_request): Parameters<RouteRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`route` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Calculate logistics route matrix",
        description = "Calculate a many-to-many route matrix for one mobility profile. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RouteMatrix>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn route_matrix(
        &self,
        Parameters(_request): Parameters<RouteMatrixRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`route_matrix` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Build optimization travel model",
        description = "Build square cost and transit-time matrices for up to 128 locations and several cuOpt vehicle types, each tied to a Map mobility profile. Unreachable pairs stay marked as unreachable. The result is an artifact that Optimization MCP reads directly. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TravelModelRecord>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn build_travel_model(
        &self,
        Parameters(_request): Parameters<BuildTravelModelRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`build_travel_model` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Calculate land reachable area",
        description = "Calculate the area reachable over the road or path network (an isochrone) for a human or road-vehicle profile. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ReachableArea>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn reachable_area(
        &self,
        Parameters(_request): Parameters<ReachableAreaRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`reachable_area` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Validate logistics route",
        description = "Check route geometry you supply: that its releases and mobility profile are still available and that no active prohibition blocks it.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RouteValidation>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn validate_route(
        &self,
        Parameters(request): Parameters<ValidateRouteRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, MapScope::Route)?;
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
        title = "Prepare Map route handoff",
        description = "Re-check a saved Map route against current mobility rules and restrictions, and return a handoff that another server, such as UAV Simulation, can execute. Fails if the route is stale or has been invalidated.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<MapRouteHandoff>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn prepare_route_handoff(
        &self,
        Parameters(request): Parameters<PrepareRouteHandoffRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, MapScope::Route)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .routes
            .prepare_route_handoff(&scope, request)
            .await
            .map_err(invalid_params)?;
        structured_result(
            format!("prepared route handoff {}", output.route_uri()),
            &output,
        )
    }

    #[tool(
        title = "Inspect logistics corridor",
        description = "Inspect a WGS84 corridor for active restrictions, facilities, boundaries, and data gaps.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CorridorInspectionOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn inspect_corridor(
        &self,
        Parameters(request): Parameters<CorridorInspectionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, MapScope::DatasetRead)?;
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
        description = "Publish a transport restriction with an explicit issuing authority and validity period. Each change creates a new version.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RestrictionMutationOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn publish_restriction(
        &self,
        Parameters(request): Parameters<PublishRestrictionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, MapScope::RestrictionPublish)?;
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
        self.state
            .subscriptions
            .notify_resource_contents_changed()
            .await;
        structured_result("published restriction".to_owned(), &output)
    }

    #[tool(
        title = "Withdraw operational restriction",
        description = "End an existing restriction and record the cancellation. Pass the revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RestrictionMutationOutput>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn withdraw_restriction(
        &self,
        Parameters(request): Parameters<WithdrawRestrictionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, MapScope::RestrictionWithdraw)?;
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

    #[tool(
        title = "Register map source",
        description = "Register a map data source. Requires the map:admin scope. Registering an identical source again has no effect.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RegisteredSource>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn register_source(
        &self,
        Parameters(request): Parameters<CreateSourceRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let scope = self.admin_scope(&context).await?;
        let source = administration::register_source(&self.state, &scope, request)
            .await
            .map_err(admin_error)?;
        structured_result(format!("registered source {}", source.source_id), &source)
    }

    #[tool(
        title = "Replace map source",
        description = "Replace a registered map source. Requires the map:admin scope. Pass the revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RegisteredSource>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn replace_source(
        &self,
        Parameters(request): Parameters<ReplaceSourceRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let scope = self.admin_scope(&context).await?;
        let source = administration::replace_source(&self.state, &scope, request)
            .await
            .map_err(admin_error)?;
        structured_result(format!("replaced source {}", source.source_id), &source)
    }

    #[tool(
        title = "Disable map source",
        description = "Disable a registered map source so no new acquisitions start from it. Requires the map:admin scope. Pass the revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RegisteredSource>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn disable_source(
        &self,
        Parameters(request): Parameters<DisableSourceRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let scope = self.admin_scope(&context).await?;
        let source = administration::disable_source(&self.state, &scope, request)
            .await
            .map_err(admin_error)?;
        structured_result(format!("disabled source {}", source.source_id), &source)
    }

    #[tool(
        title = "Start map acquisition",
        description = "Start an acquisition job that stages a dataset release for a WGS84 bounding box. Requires the map:admin scope. Poll map://acquisition/{acquisition_id} for progress.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<AcquisitionJob>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = true)
    )]
    async fn start_acquisition(
        &self,
        Parameters(request): Parameters<CreateAcquisitionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let scope = self.admin_scope(&context).await?;
        let caller = plane_caller(&context)?;
        let job = administration::start_acquisition(&self.state, scope, caller, request)
            .await
            .map_err(admin_error)?;
        structured_result(format!("started acquisition {}", job.acquisition_id), &job)
    }

    #[tool(
        title = "Cancel map acquisition",
        description = "Request cancellation of a running acquisition job. Requires the map:admin scope.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<AcquisitionJob>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn cancel_acquisition(
        &self,
        Parameters(request): Parameters<CancelAcquisitionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let scope = self.admin_scope(&context).await?;
        let job = administration::cancel_acquisition(&self.state, &scope, request)
            .await
            .map_err(admin_error)?;
        structured_result(
            format!("cancellation requested for {}", job.acquisition_id),
            &job,
        )
    }

    #[tool(
        title = "Register mobility profile",
        description = "Register a new versioned human or vehicle mobility profile. Requires the map:admin scope. Registering an identical profile again has no effect.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<MobilityProfile>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn register_mobility_profile(
        &self,
        Parameters(request): Parameters<CreateMobilityProfileRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let scope = self.admin_scope(&context).await?;
        let profile = administration::register_mobility_profile(&self.state, &scope, request)
            .await
            .map_err(admin_error)?;
        let metadata = profile.metadata();
        structured_result(
            format!(
                "registered mobility profile {} v{}",
                metadata.profile_id, metadata.version
            ),
            &profile,
        )
    }

    #[tool(
        title = "Activate dataset release",
        description = "Activate a staged dataset release, or reconcile the active one, and rebuild routing data. Requires the map:admin scope. Pass the revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ReleaseMutationResponse>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn activate_release(
        &self,
        Parameters(request): Parameters<ReleaseMutationRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let scope = self.admin_scope(&context).await?;
        let output = administration::activate_release(&self.state, &scope, request, false)
            .await
            .map_err(admin_error)?;
        structured_result(
            format!("activated release {}", output.release.release_id),
            &output,
        )
    }

    #[tool(
        title = "Roll back dataset release",
        description = "Roll back to an earlier dataset release and rebuild routing data. Requires the map:admin scope. Pass the revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ReleaseMutationResponse>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn rollback_release(
        &self,
        Parameters(request): Parameters<ReleaseMutationRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let scope = self.admin_scope(&context).await?;
        let output = administration::activate_release(&self.state, &scope, request, true)
            .await
            .map_err(admin_error)?;
        structured_result(
            format!("rolled back to release {}", output.release.release_id),
            &output,
        )
    }

    #[tool(
        title = "Quarantine dataset release",
        description = "Quarantine a dataset release that is not active and invalidate routes built from it. A quarantined release can never be activated. Requires the map:admin scope.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ReleaseMutationResponse>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn quarantine_release(
        &self,
        Parameters(request): Parameters<ReleaseMutationRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let scope = self.admin_scope(&context).await?;
        let output = administration::quarantine_release(&self.state, &scope, request)
            .await
            .map_err(admin_error)?;
        structured_result(
            format!("quarantined release {}", output.release.release_id),
            &output,
        )
    }

    async fn admin_scope(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<crate::catalog::MapAccessContext, McpError> {
        let identity = require_scope(context, MapScope::Admin)?;
        self.state.scope(&identity).await.map_err(internal)
    }
}

impl DomainServer for MapMcp {
    type Contract = MapContract;

    fn setup() -> &'static McpServerSetup<MapContract> {
        &setup::SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    // The #[tool] macro has no meta attribute; search and App links attach here.
    fn describe_tool(&self, mut tool: Tool) -> Tool {
        if tool.name == "search_locations" {
            veoveo_mcp_knowledge_extension::server::attach_search(
                &mut tool,
                &crate::knowledge::search_declaration(),
            );
        }
        if !WORKSPACE_TOOLS.contains(&tool.name.as_ref()) {
            return tool;
        }
        veoveo_mcp_apps_extension::link_tool_to_app(
            tool,
            uris::WORKSPACE_APP_URI,
            &[
                veoveo_mcp_apps_extension::UiVisibility::Model,
                veoveo_mcp_apps_extension::UiVisibility::App,
            ],
        )
    }

    async fn list_resources(
        &self,
        _declared: Vec<Resource>,
        _cursor: Option<&str>,
        context: &RequestContext<RoleServer>,
    ) -> Result<Listing<Resource>, McpError> {
        let identity = require_any_scope(context, WELL_KNOWN_SCOPES)?;
        Ok(Listing::all(discoverable_resources(
            ResourceDiscoveryAccess::from_identity(&identity),
            &self.state.workspace_basemap,
        )))
    }

    /// The host parses every address before domain policy and Store dispatch.
    async fn read(
        &self,
        address: DomainAddress<MapContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        let no_store = matches!(
            address.target(),
            crate::contract::MapTarget::KnowledgePage(_)
                | crate::contract::MapTarget::KnowledgeMember(_)
        );
        let result = self
            .read_map_resource(address.into_target(), &request.uri, context)
            .await?;
        Ok(if no_store {
            DomainRead::no_store(result)
        } else {
            DomainRead::private(result)
        })
    }

    fn prompts(&self) -> Vec<Prompt> {
        MapPrompt::ALL
            .into_iter()
            .map(MapPrompt::definition)
            .collect()
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        MapPrompt::by_name(&request.name)
            .ok_or_else(|| unknown_prompt(&request.name))?
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
        let identity = if is_feature_template(&reference.uri) {
            require_scope(&context, MapScope::FeatureRead)?
        } else {
            require_scope(&context, MapScope::DatasetRead)?
        };
        if request.argument.value.len() > 512
            || request.argument.value.chars().any(char::is_control)
        {
            return Err(invalid_params(
                "completion search text must be at most 512 bytes without control characters",
            ));
        }
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        if let Some(result) = self
            .complete_derivation(
                &identity,
                &scope,
                &reference.uri,
                &request.argument.name,
                &request.argument.value,
            )
            .await?
        {
            return Ok(result);
        }
        self.complete_index(&identity, &scope, &reference.uri, &request)
            .await
    }
}

/// Map's mutable catalogs, records and knowledge sources. Admission requires the
/// read scope of the resource's family, and knowledge members must be readable.
pub(crate) struct MapSubscriptions {
    server: MapMcp,
}

impl MapSubscriptions {
    pub(crate) fn new(server: MapMcp) -> Self {
        Self { server }
    }
}

impl ResourceSubscriptions for MapSubscriptions {
    type Address = MapAddress;

    async fn authorize(
        &self,
        addresses: Vec<MapAddress>,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        for address in addresses {
            let uri = address.to_uri();
            let uri = uri.as_str();
            if let crate::contract::MapTarget::KnowledgeMember(member) = address.into_target() {
                self.server.read_knowledge_member(member, context).await?;
            }
            if !is_subscribable(uri) {
                return Err(McpError::invalid_params(
                    "resource is immutable or not subscribable",
                    None,
                ));
            }
            if is_feature_subscribable(uri) {
                require_scope(context, MapScope::FeatureRead)?;
            } else {
                require_scope(context, MapScope::DatasetRead)?;
                if uri == uris::SPATIAL_DERIVATIONS_URI {
                    require_scope(context, MapScope::SpatialDerive)?;
                }
            }
        }
        Ok(())
    }

    fn hub(&self) -> &SubscriptionHub {
        self.server.state.subscriptions.as_ref()
    }
}

fn identity_has_scope(identity: &GatewayInternalIdentity, required: MapScope) -> bool {
    setup::SERVER_SETUP.has_scope(&identity.actor.scopes, required)
}

fn admin_error(error: AdminOpError) -> McpError {
    match error {
        AdminOpError::BadRequest(message) => McpError::invalid_params(message, None),
        AdminOpError::Conflict(message) => McpError::invalid_params(message, None),
        AdminOpError::NotFound(message) => McpError::resource_not_found(message, None),
        AdminOpError::Internal(error) => {
            tracing::error!("Map administrative operation failed: {error:#}");
            McpError::internal_error("Map administrative operation failed", None)
        }
    }
}

fn require_scope(
    context: &RequestContext<RoleServer>,
    required: MapScope,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = gateway_identity(context)?;
    crate::server::auth::require_scope(&identity.actor.scopes, required)?;
    Ok(identity)
}

fn require_any_scope(
    context: &RequestContext<RoleServer>,
    required: &[MapScope],
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = gateway_identity(context)?;
    if !required
        .iter()
        .any(|required| identity_has_scope(&identity, *required))
    {
        return Err(McpError::invalid_request(
            format!(
                "You don't have permission to make this request. It needs one of these scopes: {}.",
                required
                    .iter()
                    .map(|scope| scope.name().as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None,
        ));
    }
    Ok(identity)
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

/// Well-known surface resources (contract C18, C19). `list_resources` serves
/// these for every authorized identity and `stable_resource_uris` declares
/// them in the `map://contract` capability inventory, so the two cannot
/// diverge.
fn is_subscribable(uri: &str) -> bool {
    if crate::contract::MapKnowledgePageUri::parse(uri).is_ok()
        || crate::contract::MapKnowledgeMember::parse(uri).is_ok()
    {
        return true;
    }
    matches!(
        uri,
        uris::ACTIVE_RELEASES_URI
            | uris::DATASETS_URI
            | uris::MOBILITY_PROFILES_URI
            | uris::RESTRICTIONS_URI
            | uris::ROUTES_URI
            | uris::TRAVEL_MODELS_URI
            | uris::RASTER_DERIVATIONS_URI
            | uris::SPATIAL_DERIVATIONS_URI
    ) || crate::contract::MapMobilityProfileUri::parse(uri).is_ok()
        || crate::contract::MapRestrictionUri::parse(uri).is_ok()
        || crate::contract::MapRouteUri::parse(uri).is_ok()
        || uris::parse_dataset(uri).is_some()
        || is_feature_subscribable(uri)
}

fn is_feature_subscribable(uri: &str) -> bool {
    if let Ok(page) = crate::contract::MapKnowledgePageUri::parse(uri) {
        return page.collection().scope() == MapScope::FeatureRead;
    }
    if let Ok(member) = crate::contract::MapKnowledgeMember::parse(uri) {
        return member.collection().scope() == MapScope::FeatureRead;
    }
    matches!(
        uri,
        uris::FEATURE_LAYERS_URI
            | uris::PUBLICATIONS_URI
            | uris::LAYER_PRODUCTS_URI
            | uris::COMPOSITIONS_URI
    ) || uris::parse_feature_layer(uri).is_some()
        || uris::parse_features(uri).is_some()
        || uris::parse_feature(uri).is_some()
        || uris::parse_composition(uri).is_some()
}

fn is_feature_template(uri: &str) -> bool {
    uri.starts_with("map://feature-layer/")
        || uri.starts_with("map://feature-style/")
        || uri.starts_with("map://composition/")
}

#[cfg(test)]
#[path = "mcp/well_known_tests.rs"]
mod well_known_tests;

#[cfg(test)]
mod tool_input_tests;
#[cfg(test)]
#[path = "mcp/workspace_app_tests.rs"]
mod workspace_app_tests;
