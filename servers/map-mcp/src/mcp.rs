use std::sync::{Arc, LazyLock};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, CancelTaskParams,
        CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock,
        GetPromptRequestParams, GetTaskParams, GetTaskResult, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        Prompt, ReadResourceRequestParams, ReadResourceResult, Reference, Resource,
        ResourceContents, ResourceTemplate, ServerCapabilities, ServerConfig, SubscriptionFilter,
        UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
};
use serde::Serialize;
use veoveo_mcp_contract::{GatewayInternalIdentity, Page, PlaneCaller, docs::ServerDocs, paginate};
use veoveo_types::ScopeDefinition;

use crate::{
    administration::{self, AdminOpError},
    contract::{
        AcquisitionId, AcquisitionJob, BuildTravelModelRequest, CancelAcquisitionRequest,
        CorridorInspectionOutput, CorridorInspectionRequest, CreateAcquisitionRequest,
        CreateMobilityProfileRequest, CreateSourceRequest, DeriveRasterRequest,
        DeriveSpatialGeometryRequest, DisableSourceRequest, FacilityId, GeodesicDirectOutput,
        GeodesicDirectRequest, GeodesicInverseOutput, GeodesicInverseRequest,
        InspectLocationOutput, InspectLocationRequest, InspectPositionOutput,
        InspectPositionRequest, ListActiveDatasetReleasesOutput, ListActiveDatasetReleasesRequest,
        LocationId, MapDatasetId, MapRouteHandoff, MapScope, MobilityProfile, MobilityProfileId,
        PrepareRouteHandoffRequest, PublishRestrictionRequest, QuerySourceFeaturesOutput,
        QuerySourceFeaturesRequest, RasterDerivation, ReachableArea, ReachableAreaRequest,
        RegisteredSource, ReleaseMutationRequest, ReleaseMutationResponse, ReplaceSourceRequest,
        RestrictionMutationOutput, RouteMatrix, RouteMatrixId, RouteMatrixRequest, RoutePlan,
        RouteRequest, RouteValidation, SearchLocationsOutput, SearchLocationsRequest,
        SpatialDerivation, TransformCrsOutput, TransformCrsRequest, TravelModelRecord,
        ValidateGeofenceOutput, ValidateGeofenceRequest, ValidateRouteRequest,
        WithdrawRestrictionRequest,
    },
    geodesy,
    prompts::MapPrompt,
    server::{auth::ForwardedBearer, tasks::MapTaskExtension},
    state::MapApplication,
    uris,
};

mod authoring;
mod completion;
mod derivations;
mod discovery;
mod resources;
#[cfg(test)]
use discovery::stable_resource_uris;
use discovery::{ResourceDiscoveryAccess, discoverable_resources, resource_templates};
mod metadata;
mod owned;
mod releases;

const LIST_PAGE_SIZE: usize = 100;

/// The crate documents embedded at build time and served under the well-known
/// surface: `map://docs`, `map://docs/{doc_id}`, `map://contract`, and the
/// administrative `admin/docs` routes (contract C18-C21).
pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> = LazyLock::new(|| {
    veoveo_mcp_contract::server_docs!("map")
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
    task_service: MapTaskExtension,
    #[allow(dead_code)]
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
            task_service: MapTaskExtension::new(state.clone()),
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
        output_schema = rmcp::handler::server::tool::schema_for_type::<SearchLocationsOutput>(),
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
        structured_result(
            format!("found {} location(s)", output.locations.len()),
            &output,
        )
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
        let pointers = self
            .state
            .catalog
            .list_active_releases(&scope)
            .await
            .map_err(internal)?;
        let mut releases = Vec::new();
        for pointer in pointers {
            if request
                .dataset_id
                .as_ref()
                .is_some_and(|dataset_id| *dataset_id != pointer.dataset_id)
            {
                continue;
            }
            let release = self
                .state
                .catalog
                .release(&scope, &pointer.release_id)
                .await
                .map_err(internal)?
                .ok_or_else(|| internal("active dataset release is missing"))?;
            if request
                .source_id
                .as_ref()
                .is_some_and(|source_id| *source_id != release.source_id)
            {
                continue;
            }
            releases.push(crate::contract::ActiveDatasetRelease { pointer, release });
        }
        let truncated = releases.len() > request.limit as usize;
        releases.truncate(request.limit as usize);
        let output = ListActiveDatasetReleasesOutput {
            releases,
            truncated,
        };
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
            format!("prepared route handoff {}", output.route_uri),
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
        let caller = internal_caller(&context)?;
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

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MapMcp {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }

    fn get_info(&self) -> ServerConfig {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_completions()
            .build();
        veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
        SERVER_DOCS.declare_knowledge(&mut capabilities);
        capabilities.extensions.get_or_insert_default().insert(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            rmcp::model::JsonObject::new(),
        );
        let mut info = ServerConfig::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new("map", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Geography, your own feature layers, and route planning for people, road and off-road vehicles, rail, maritime, and aviation. For routes, call `route` or `route_matrix` as MCP Tasks with an explicit mobility profile and departure time. To feed Optimization MCP, call `build_travel_model`. A route with `planning_advisory` status is guidance, not a certified plan. For your own data, create GeoJSON/JSON-FG feature layers in your Work Context, edit them with changesets, query them with CQL2 JSON, and publish fixed versions. Import, export, GeoPackage inspection, and vector-tile builds run as MCP Tasks. Your features never affect routing. The ui://map/workspace.html app shows compositions and layers interactively. Managing sources, acquisitions, releases, and mobility profiles requires the map:admin scope."
                .to_owned(),
        );
        info
    }

    async fn call_tool(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if let Some(created) =
            veoveo_task_runtime::start_durable_tool_task(&self.task_service, &mut request, &context)
                .await?
        {
            return Ok(created.into());
        }
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(call).await
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::get_task(&self.task_service, &caller, request)
            .await
    }

    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::update_task(&self.task_service, &caller, request)
            .await
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::cancel_task(
            &self.task_service,
            &caller,
            request.task_id,
        )
        .await
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        // The #[tool] macro has no meta attribute; app links attach here.
        tools = tools
            .into_iter()
            .map(|tool| {
                if WORKSPACE_TOOLS.contains(&tool.name.as_ref()) {
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        tool,
                        uris::WORKSPACE_APP_URI,
                        &[
                            veoveo_mcp_apps_extension::UiVisibility::Model,
                            veoveo_mcp_apps_extension::UiVisibility::App,
                        ],
                    )
                } else {
                    tool
                }
            })
            .collect();
        let page = mcp_page(tools, request.as_ref())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let identity = require_any_scope(&context, WELL_KNOWN_SCOPES)?;
        let resources = discoverable_resources(
            ResourceDiscoveryAccess::from_identity(&identity),
            &self.state.workspace_basemap,
        );
        let page = mcp_page(resources, request.as_ref())?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let page = mcp_page(resource_templates(), request.as_ref())?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        if let Some(result) = SERVER_DOCS.read_knowledge(
            &veoveo_types::ResourceScheme::new("map").expect("declared scheme"),
            &request,
            &context,
        )? {
            return Ok(result);
        }
        self.read_map_resource(request, context).await
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
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::GetPromptResponse, McpError> {
        async {
            MapPrompt::by_name(&request.name)
                .ok_or_else(|| McpError::invalid_params("unknown Map prompt", None))?
                .render(request.arguments)
        }
        .await
        .map(Into::into)
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

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let request_context = context.request_context().clone();
        for uri in context.accepted().resource_subscriptions.iter().flatten() {
            if !is_subscribable(uri) {
                return Err(McpError::invalid_params(
                    "resource is immutable or not subscribable",
                    None,
                ));
            }
            if is_feature_subscribable(uri) {
                require_scope(&request_context, MapScope::FeatureRead)?;
            } else {
                require_scope(&request_context, MapScope::DatasetRead)?;
                if uri == uris::SPATIAL_DERIVATIONS_URI {
                    require_scope(&request_context, MapScope::SpatialDerive)?;
                }
            }
        }
        veoveo_task_runtime::listen_durable_subscriptions(
            &self.task_service,
            context,
            Some(self.state.subscriptions.as_ref()),
            None,
        )
        .await
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
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })
}

fn internal_caller(context: &RequestContext<RoleServer>) -> Result<PlaneCaller, McpError> {
    let identity = internal_identity(context)?;
    let bearer_token = context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<ForwardedBearer>())
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    let memberships = identity.actor.group_memberships();
    Ok(PlaneCaller {
        identity,
        memberships,
        bearer_token,
    })
}

fn identity_has_scope(identity: &GatewayInternalIdentity, required: MapScope) -> bool {
    identity.actor.scopes.contains(required.name())
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
    let identity = internal_identity(context)?;
    crate::server::auth::require_scope(&identity.actor.scopes, required)?;
    Ok(identity)
}

fn require_any_scope(
    context: &RequestContext<RoleServer>,
    required: &[MapScope],
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
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

/// Well-known surface resources (contract C18, C19). `list_resources` serves
/// these for every authorized identity and `stable_resource_uris` declares
/// them in the `map://contract` capability inventory, so the two cannot
/// diverge.
fn is_subscribable(uri: &str) -> bool {
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
        || uris::parse_single(uri, "map://dataset/").is_some()
        || is_feature_subscribable(uri)
}

fn is_feature_subscribable(uri: &str) -> bool {
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
mod well_known_tests {
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::{
        ResourceDiscoveryAccess, SERVER_DOCS, discoverable_resources, stable_resource_uris,
    };
    use crate::{contract::MapWorkspaceBasemap, uris};

    #[test]
    fn derivation_collections_accept_the_notifications_the_workers_emit() {
        assert!(super::is_subscribable(uris::RASTER_DERIVATIONS_URI));
        assert!(super::is_subscribable(uris::SPATIAL_DERIVATIONS_URI));
        assert!(!super::is_subscribable("map://spatial-derivations/extra"));
        assert!(!super::is_subscribable(uris::DOCS_URI));
    }

    #[test]
    fn subscription_addresses_apply_the_resource_owners_admission() {
        let route = crate::contract::MapRouteUri::new(crate::contract::RouteId::new());
        let restriction =
            crate::contract::MapRestrictionUri::new(crate::contract::RestrictionId::new());
        for address in [route.as_str(), restriction.as_str()] {
            assert!(super::is_subscribable(address));
            assert!(!super::is_subscribable(&format!("{address}?secret=value")));
            assert!(!super::is_subscribable(&format!("{address}#fragment")));
        }
        assert!(!super::is_subscribable("map://route/arbitrary"));
        assert!(!super::is_subscribable("map://restriction/arbitrary"));
    }

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "map");
        let agents = SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
        assert!(agents.body.contains("## Contract Compliance"));
        let design = SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
        assert!(!design.body.is_empty());
        let index = SERVER_DOCS.llms_txt();
        assert!(index.contains("(agents)"));
        assert!(index.contains("(design)"));
        for id in ["authoring", "acquisition", "routing"] {
            assert!(SERVER_DOCS.doc(id).is_some(), "missing Map document {id}");
        }
        for document in SERVER_DOCS.iter() {
            // Leave space for the observation and the kernel's provenance line.
            assert!(
                document.body.len() + 1024 <= 64 * 1024,
                "Map document {} exceeds the knowledge item budget",
                document.id
            );
        }
    }

    #[test]
    fn contract_declaration_resolves_from_the_embedded_manual() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        assert_eq!(declaration.server, "map");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C17", "C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        let json = serde_json::to_value(&declaration).expect("declaration serializes");
        assert_eq!(json["server"], "map");
    }

    #[test]
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }

    #[test]
    fn resource_discovery_is_bounded_by_the_protocol_surface() {
        let basemap = MapWorkspaceBasemap::open_free_map(
            "https://tiles.openfreemap.org/styles/positron",
            "https://tiles.openfreemap.org/styles/dark",
        )
        .unwrap();
        let resources = discoverable_resources(
            ResourceDiscoveryAccess {
                admin: true,
                dataset_read: true,
                feature_read: true,
                spatial_derive: true,
            },
            &basemap,
        );
        assert_eq!(resources.len(), stable_resource_uris().len());
        assert!(resources.len() < 32);
        assert!(resources.windows(2).all(|pair| pair[0].uri < pair[1].uri));
        assert!(
            resources
                .iter()
                .any(|resource| resource.uri == uris::DATASETS_URI)
        );
        assert!(
            resources
                .iter()
                .all(|resource| !resource.uri.starts_with("map://release/"))
        );
    }
}

#[cfg(test)]
mod workspace_app_tests {
    use super::{MapMcp, ResourceDiscoveryAccess, discoverable_resources};
    use crate::{contract::MapWorkspaceBasemap, uris};

    fn basemap() -> MapWorkspaceBasemap {
        MapWorkspaceBasemap::open_free_map(
            "https://tiles.openfreemap.org/styles/positron",
            "https://tiles.openfreemap.org/styles/dark",
        )
        .unwrap()
    }

    #[test]
    fn workspace_tools_exist_in_the_canonical_router() {
        let tools = MapMcp::full_tool_router().list_all();
        assert!(!tools.is_empty());
        for expected in super::WORKSPACE_TOOLS {
            assert!(
                tools.iter().any(|tool| tool.name.as_ref() == *expected),
                "workspace tool {expected} is absent from the canonical router"
            );
        }
    }

    const WORKSPACE_APP: &str = include_str!("../assets/workspace-app.html");

    #[test]
    fn workspace_uses_sans_serif_typography() {
        assert!(WORKSPACE_APP.contains("--sans:"));
        for obsolete_family in [
            "--serif",
            "ui-serif",
            "Iowan Old Style",
            "Palatino",
            "Georgia",
        ] {
            assert!(
                !WORKSPACE_APP.contains(obsolete_family),
                "workspace retains obsolete serif family {obsolete_family}"
            );
        }
    }

    #[test]
    fn workspace_applies_host_context_and_uses_only_the_mcp_bridge() {
        assert!(WORKSPACE_APP.contains("ui/initialize"));
        assert!(WORKSPACE_APP.contains("ui/notifications/host-context-changed"));
        assert!(WORKSPACE_APP.contains("resources/read"));
        assert!(WORKSPACE_APP.contains("tools/call"));
        assert!(!WORKSPACE_APP.contains("<script src="));
        assert!(!WORKSPACE_APP.contains("<link href="));
        for external_reference in [
            "src=\"http://",
            "src=\"https://",
            "href=\"http://",
            "href=\"https://",
            "url(http://",
            "url(https://",
            "@import",
        ] {
            assert!(
                !WORKSPACE_APP
                    .to_ascii_lowercase()
                    .contains(external_reference),
                "workspace contains external fetch reference {external_reference}"
            );
        }
    }

    #[test]
    fn workspace_is_permission_aware() {
        assert!(WORKSPACE_APP.contains("map://workspace"));
        for capability in [
            "administration",
            "dataset_read",
            "feature_read",
            "feature_write",
            "feature_publish",
        ] {
            assert!(WORKSPACE_APP.contains(capability));
        }
    }

    #[test]
    fn workspace_preserves_admin_and_authoring_operations() {
        for tool in [
            "register_source",
            "start_acquisition",
            "activate_release",
            "register_mobility_profile",
            "create_feature_layer",
            "validate_feature_changes",
            "commit_feature_changes",
            "query_features",
            "query_source_features",
            "publish_feature_layer",
            "create_map_composition",
            "inspect_geopackage",
            "import_feature_layer",
        ] {
            assert!(WORKSPACE_APP.contains(tool), "workspace is missing {tool}");
        }
    }

    #[test]
    fn workspace_is_a_persistent_hardware_map_with_bounded_synchronized_previews() {
        for marker in [
            "hardware-backed WebGL2",
            "WEBGL_debug_renderer_info",
            "swiftshader",
            "publication_id",
            "query_features",
            "query_source_features",
            "Persistent map",
            "Data preview",
            "preview cap reached",
            "light_style_url",
            "dark_style_url",
            "subscriptions/listen",
            "maplibre-gl@6.6.0",
            "maplibre-worker.cjs",
            "renderedFeatureCount",
            "without painting any returned feature",
        ] {
            assert!(
                WORKSPACE_APP.contains(marker),
                "workspace is missing {marker}"
            );
        }
    }

    #[test]
    fn one_workspace_is_discoverable_for_dataset_feature_or_admin_access() {
        for access in [
            ResourceDiscoveryAccess {
                admin: true,
                dataset_read: false,
                feature_read: false,
                spatial_derive: false,
            },
            ResourceDiscoveryAccess {
                admin: false,
                dataset_read: false,
                feature_read: true,
                spatial_derive: false,
            },
            ResourceDiscoveryAccess {
                admin: false,
                dataset_read: true,
                feature_read: false,
                spatial_derive: false,
            },
        ] {
            let apps = discoverable_resources(access, &basemap())
                .into_iter()
                .filter(|resource| resource.uri.starts_with("ui://"))
                .collect::<Vec<_>>();
            assert_eq!(apps.len(), 1);
            assert_eq!(apps[0].uri, uris::WORKSPACE_APP_URI);
        }
        let workspace = discoverable_resources(
            ResourceDiscoveryAccess {
                admin: false,
                dataset_read: false,
                feature_read: true,
                spatial_derive: false,
            },
            &basemap(),
        )
        .into_iter()
        .find(|resource| resource.uri == uris::WORKSPACE_APP_URI)
        .expect("map workspace is discoverable");
        let metadata = veoveo_mcp_apps_extension::resource_ui_meta(&workspace)
            .expect("map workspace UI metadata is valid");
        assert_eq!(metadata.prefers_border, None);
        let csp = metadata.csp.unwrap();
        assert_eq!(csp.connect_domains, ["https://tiles.openfreemap.org"]);
        assert_eq!(csp.resource_domains, ["https://tiles.openfreemap.org"]);
    }
}
