use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock},
};

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
    tool_handler, tool_router,
};
use serde::Serialize;
use serde_json::json;
use veoveo_mcp_contract::tool;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, Page, PlaneCaller,
    docs::{CapabilityInventory, ContractDeclaration, ServerDocs},
    paginate,
};

use crate::{
    administration::{self, AdminOpError},
    contract::{
        AcquisitionId, AcquisitionJob, CancelAcquisitionRequest, CorridorInspectionOutput,
        CorridorInspectionRequest, CreateAcquisitionRequest, CreateMobilityProfileRequest,
        CreateSourceRequest, DatasetReleaseId, DisableSourceRequest, FacilityId,
        GeodesicDirectOutput, GeodesicDirectRequest, GeodesicInverseOutput, GeodesicInverseRequest,
        InspectLocationOutput, InspectLocationRequest, LocationId, MapDatasetId, MapSourceId,
        MobilityProfile, MobilityProfileId, PublishRestrictionRequest, ReachableArea,
        ReachableAreaRequest, RegisteredSource, ReleaseMutationRequest, ReleaseMutationResponse,
        ReplaceSourceRequest, RestrictionId, RestrictionMutationOutput, RouteId, RouteMatrix,
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

mod authoring;

const LIST_PAGE_SIZE: usize = 100;

/// The crate documents embedded at build time and served under the well-known
/// surface: `map://docs`, `map://docs/{doc_id}`, `map://contract`, and the
/// administrative `admin/docs` routes (contract C18-C21).
pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("map"));

/// Scopes that may read the well-known surface; the same set gates
/// `list_resources`, so any identity able to list resources can read the
/// server's manual and contract declaration.
const WELL_KNOWN_SCOPES: &[&str] = &["map:dataset:read", "map:feature:read", "map:admin"];

/// Tools the map administration app may invoke; each is linked to the app
/// view in `list_tools` and scope-gated to `map:admin` in its handler.
const ADMIN_TOOLS: &[&str] = &[
    "register_source",
    "replace_source",
    "disable_source",
    "start_acquisition",
    "cancel_acquisition",
    "register_mobility_profile",
    "activate_release",
    "rollback_release",
    "quarantine_release",
];

const EDITOR_TOOLS: &[&str] = &[
    "archive_feature_layer",
    "archive_map_composition",
    "build_vector_tiles",
    "commit_feature_changes",
    "create_feature_layer",
    "create_map_composition",
    "export_feature_layer",
    "import_feature_layer",
    "publish_feature_layer",
    "query_features",
    "restore_feature",
    "update_feature_layer",
    "update_map_composition",
    "validate_feature_changes",
];

/// Self-contained icon for the admin app (lucide `map-pinned` outline).
const ADMIN_APP_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNCIgaGVpZ2h0PSIyNCIgdmlld0JveD0iMCAwIDI0IDI0IiBmaWxsPSJub25lIiBzdHJva2U9IiM0YTdkZDYiIHN0cm9rZS13aWR0aD0iMiIgc3Ryb2tlLWxpbmVjYXA9InJvdW5kIiBzdHJva2UtbGluZWpvaW49InJvdW5kIj48cGF0aCBkPSJNMTggOGMwIDMuNjEzLTMuODY5IDcuNDI5LTUuMzkzIDguNzk1YTEgMSAwIDAgMS0xLjIxNCAwQzkuODcgMTUuNDI5IDYgMTEuNjEzIDYgOGE2IDYgMCAwIDEgMTIgMCIvPjxjaXJjbGUgY3g9IjEyIiBjeT0iOCIgcj0iMiIvPjxwYXRoIGQ9Ik04LjcxNCAxNGgtMy43MWExIDEgMCAwIDAtLjk0OC42ODNsLTIuMDA0IDZBMSAxIDAgMCAwIDMgMjJoMThhMSAxIDAgMCAwIC45NDgtMS4zMTZsLTItNmExIDEgMCAwIDAtLjk0OS0uNjg0aC0zLjcxMiIvPjwvc3ZnPg==";

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
    /// Tools and prompts derive from the live registrations. Stable resources,
    /// resource templates, and task-augmented tool names come from the lists
    /// those surfaces are served from (`stable_resource_uris`,
    /// `resource_templates`, `crate::server::tasks::TASK_TOOLS`), so the
    /// declaration cannot silently diverge from what the handler registers.
    pub(crate) fn capability_inventory() -> CapabilityInventory {
        let mut tools: Vec<String> = Self::full_tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();
        tools.sort();
        let mut prompts: Vec<String> = MapPrompt::ALL
            .into_iter()
            .map(|prompt| prompt.definition().name)
            .collect();
        prompts.sort();
        CapabilityInventory {
            tools,
            resources: stable_resource_uris(),
            resource_templates: resource_templates()
                .into_iter()
                .map(|template| template.uri_template.clone())
                .collect(),
            prompts,
            tasks: crate::server::tasks::TASK_TOOLS
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
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
        description = "Calculate a governed route for one versioned human or vehicle mobility profile through durable task invocation. The result pins releases, restrictions, a snapshot, costs, and validation state; unavailable coverage is never replaced by a straight line.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RoutePlan>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn route(
        &self,
        Parameters(_request): Parameters<RouteRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "route requires task-based invocation",
            None,
        ))
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
        veoveo_mcp_contract::notify_resource_list_changed(&context.peer).await;
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

    #[tool(
        title = "Register map source",
        description = "Register a governed authoritative map source. Requires the map:admin scope; idempotent on identical re-registration.",
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
        description = "Replace a registered map source under optimistic concurrency. Requires the map:admin scope.",
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
        description = "Disable a registered map source under optimistic concurrency so no new acquisitions can start from it. Requires the map:admin scope.",
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
        description = "Start a governed acquisition job that stages a dataset release for an explicit WGS84 coverage box. Requires the map:admin scope. Poll map://acquisition/{acquisition_id} for progress.",
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
        description = "Register a new versioned human or vehicle mobility profile. Requires the map:admin scope; idempotent on identical re-registration.",
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
        description = "Activate a staged dataset release (or reconcile the current active release) under optimistic concurrency, rebuilding routing products. Requires the map:admin scope.",
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
        description = "Roll the active pointer back to an earlier release under optimistic concurrency, rebuilding routing products. Requires the map:admin scope.",
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
        description = "Quarantine a non-active dataset release and invalidate routes derived from it. Quarantined releases can never be activated. Requires the map:admin scope.",
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
    ) -> Result<crate::catalog::MapScope, McpError> {
        let identity = require_scope(context, "map:admin")?;
        self.state.scope(&identity).await.map_err(internal)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for MapMcp {
    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new("map", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Earth geography, governed authored feature layers, and logistics planning for human, road, off-road, rail, maritime, and aviation mobility. Create and revise Work Context-owned GeoJSON/JSON-FG features with optimistic changesets, query their DuckDB Spatial projection through bounded CQL2 JSON, and publish immutable layer revisions. Use durable tasks to import an authorized GeoJSON artifact, export a publication as GeoJSON Sequence or GeoParquet 1.0, or build an MVT 2.1 bundle. Compose publication pins through map://composition resources or the ui://map/editor.html MCP App. Generic authored features never affect routing until a separate governed promotion validates them into a routing dataset release. Read versioned map:// resources, invoke route or route_matrix through the Task API with an explicit profile and departure time, and treat planning_advisory status as non-certified guidance. Source, acquisition, release, and mobility-profile administration runs through the map:admin-scoped tools and the ui://map/admin.html app view."
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
        // The #[tool] macro has no meta attribute; app links attach here.
        tools = tools
            .into_iter()
            .map(|tool| {
                if ADMIN_TOOLS.contains(&tool.name.as_ref()) {
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        tool,
                        uris::ADMIN_APP_URI,
                        &[
                            veoveo_mcp_apps_extension::UiVisibility::Model,
                            veoveo_mcp_apps_extension::UiVisibility::App,
                        ],
                    )
                } else if EDITOR_TOOLS.contains(&tool.name.as_ref()) {
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        tool,
                        uris::EDITOR_APP_URI,
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
            meta: None,
        })
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let identity = internal_identity(&context)?;
        if !identity_has_scope(&identity, "map:dataset:read")
            && !identity_has_scope(&identity, "map:feature:read")
            && !identity_has_scope(&identity, "map:admin")
        {
            return Err(McpError::invalid_request(
                "scope `map:dataset:read` or `map:feature:read` is required",
                None,
            ));
        }
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let mut resources = Vec::new();
        resources.extend(well_known_resources());
        if identity_has_scope(&identity, "map:admin") {
            resources.push(
                veoveo_mcp_apps_extension::app_resource(uris::ADMIN_APP_URI, "map-admin-app")
                    .with_title("Map data")
                    .with_description(
                        "Interactive MCP App governing map sources, acquisitions, dataset \
                         releases, and mobility profiles.",
                    )
                    .with_icons(vec![rmcp::model::Icon::new(ADMIN_APP_ICON)]),
            );
            resources.push(json_resource_descriptor(
                uris::ACQUISITIONS_URI.to_owned(),
                "Map acquisitions".to_owned(),
                "Governed acquisition jobs (map:admin).",
            ));
            resources.push(json_resource_descriptor(
                uris::ACTIVE_RELEASES_URI.to_owned(),
                "Active releases".to_owned(),
                "Active dataset release pointers (map:admin).",
            ));
        }
        if identity_has_scope(&identity, "map:dataset:read") {
            resources.extend(root_resources());
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
        }
        if identity_has_scope(&identity, "map:feature:read") {
            resources.push(
                veoveo_mcp_apps_extension::app_resource(uris::EDITOR_APP_URI, "map-editor-app")
                    .with_title("Map feature editor")
                    .with_description(
                        "Interactive MCP App for governed feature layers, changesets, publications, and compositions.",
                    )
                    .with_icons(vec![rmcp::model::Icon::new(ADMIN_APP_ICON)]),
            );
            resources.push(json_resource_descriptor(
                uris::FEATURE_LAYERS_URI.to_owned(),
                "Authored feature layers".to_owned(),
                "Work Context-scoped mutable layer heads and immutable revision links.",
            ));
            resources.push(json_resource_descriptor(
                uris::PUBLICATIONS_URI.to_owned(),
                "Feature layer publications".to_owned(),
                "Immutable published layer revisions.",
            ));
            resources.push(json_resource_descriptor(
                uris::LAYER_PRODUCTS_URI.to_owned(),
                "Feature layer products".to_owned(),
                "Immutable artifacts derived from published feature layers.",
            ));
            resources.push(json_resource_descriptor(
                uris::COMPOSITIONS_URI.to_owned(),
                "Map compositions".to_owned(),
                "Work Context-scoped maps built from immutable publication pins.",
            ));
            for layer in self
                .state
                .authoring
                .list_layers(&identity, &scope, false)
                .await
                .map_err(internal)?
            {
                resources.push(json_resource_descriptor(
                    uris::feature_layer_uri(layer.layer_id.as_str()),
                    layer.title,
                    "Governed authored feature layer.",
                ));
            }
            for publication in self
                .state
                .authoring
                .list_publications(&identity, &scope, None)
                .await
                .map_err(internal)?
            {
                resources.push(json_resource_descriptor(
                    uris::publication_uri(
                        publication.layer_id.as_str(),
                        publication.publication_id.as_str(),
                    ),
                    publication
                        .title
                        .unwrap_or_else(|| format!("publication {}", publication.publication_id)),
                    "Immutable authored feature layer publication.",
                ));
            }
            for product in self
                .state
                .authoring
                .list_layer_products(&identity, &scope, None)
                .await
                .map_err(internal)?
            {
                resources.push(json_resource_descriptor(
                    uris::layer_product_uri(
                        product.layer_id.as_str(),
                        product.publication_id.as_str(),
                        product.product_id.as_str(),
                    ),
                    format!("{:?} product {}", product.format, product.product_id),
                    "Immutable artifact derived from a layer publication.",
                ));
            }
            for composition in self
                .state
                .authoring
                .list_compositions(&identity, &scope, false)
                .await
                .map_err(internal)?
            {
                resources.push(json_resource_descriptor(
                    uris::composition_uri(composition.composition_id.as_str()),
                    composition.title,
                    "Governed map composition head.",
                ));
            }
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
        let page = mcp_page(resource_templates(), request.as_ref())?;
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
        let uri = request.uri.as_str();
        // Well-known surface (contract C18, C19): readable by any identity
        // that can list resources.
        if uri == uris::DOCS_URI {
            require_any_scope(&context, WELL_KNOWN_SCOPES)?;
            return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
        }
        if let Some(doc_id) = uris::parse_doc(uri) {
            require_any_scope(&context, WELL_KNOWN_SCOPES)?;
            let doc = SERVER_DOCS
                .doc(doc_id)
                .ok_or_else(|| not_found("server document"))?;
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
            ]));
        }
        if uri == uris::CONTRACT_URI {
            require_any_scope(&context, WELL_KNOWN_SCOPES)?;
            return json_resource(
                uri,
                &ContractDeclaration::from_docs(&SERVER_DOCS, Self::capability_inventory()),
            );
        }
        // Administration resources carry the admin scope, not dataset read.
        if uri == uris::ADMIN_APP_URI {
            require_scope(&context, "map:admin")?;
            return Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(
                    uri,
                    include_str!("../assets/admin-app.html"),
                ),
            ]));
        }
        if uri == uris::EDITOR_APP_URI {
            require_scope(&context, "map:feature:read")?;
            return Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(
                    uri,
                    include_str!("../assets/editor-app.html"),
                ),
            ]));
        }
        if uri == uris::ACQUISITIONS_URI {
            let identity = require_scope(&context, "map:admin")?;
            let scope = self.state.scope(&identity).await.map_err(internal)?;
            self.state
                .acquisitions
                .reconcile_interrupted(&scope)
                .await
                .map_err(internal)?;
            let jobs = self
                .state
                .catalog
                .list_acquisitions(&scope)
                .await
                .map_err(internal)?;
            return json_resource(uri, &jobs);
        }
        if uri == uris::ACTIVE_RELEASES_URI {
            let identity = require_scope(&context, "map:admin")?;
            let scope = self.state.scope(&identity).await.map_err(internal)?;
            let pointers = self
                .state
                .catalog
                .list_active_releases(&scope)
                .await
                .map_err(internal)?;
            return json_resource(uri, &pointers);
        }
        if let Some(value) = uris::parse_single(uri, "map://acquisition/") {
            let identity = require_scope(&context, "map:admin")?;
            let scope = self.state.scope(&identity).await.map_err(internal)?;
            let id = AcquisitionId::parse(value).map_err(invalid_params)?;
            let job = self
                .state
                .catalog
                .acquisition(&scope, &id)
                .await
                .map_err(internal)?
                .ok_or_else(|| not_found("acquisition"))?;
            return json_resource(uri, &job);
        }
        if uri == uris::FEATURE_LAYERS_URI
            || uri == uris::PUBLICATIONS_URI
            || uri == uris::LAYER_PRODUCTS_URI
            || uri == uris::COMPOSITIONS_URI
            || uri.starts_with("map://feature-layer/")
            || uri.starts_with("map://composition/")
        {
            let identity = require_scope(&context, "map:feature:read")?;
            let scope = self.state.scope(&identity).await.map_err(internal)?;
            if uri == uris::FEATURE_LAYERS_URI {
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .list_layers(&identity, &scope, false)
                        .await
                        .map_err(internal)?,
                );
            }
            if uri == uris::PUBLICATIONS_URI {
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .list_publications(&identity, &scope, None)
                        .await
                        .map_err(internal)?,
                );
            }
            if uri == uris::LAYER_PRODUCTS_URI {
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .list_layer_products(&identity, &scope, None)
                        .await
                        .map_err(internal)?,
                );
            }
            if uri == uris::COMPOSITIONS_URI {
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .list_compositions(&identity, &scope, false)
                        .await
                        .map_err(internal)?,
                );
            }
            if let Some((composition, revision)) = uris::parse_composition_revision(uri) {
                let composition_id: crate::contract::MapCompositionId =
                    composition.parse().map_err(invalid_params)?;
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .composition_revision(&identity, &scope, &composition_id, revision)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("map composition revision"))?,
                );
            }
            if let Some(composition) = uris::parse_composition(uri) {
                let composition_id: crate::contract::MapCompositionId =
                    composition.parse().map_err(invalid_params)?;
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .composition(&identity, &scope, &composition_id)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("map composition"))?,
                );
            }
            if let Some(request) = uris::parse_features_request(uri).map_err(invalid_params)? {
                let output = self
                    .state
                    .authoring
                    .query_features(&identity, &scope, request)
                    .await
                    .map_err(invalid_params)?;
                return json_resource(uri, &output);
            }
            if let Some((layer, feature, revision)) = uris::parse_feature_revision(uri) {
                let layer_id: crate::contract::FeatureLayerId =
                    layer.parse().map_err(invalid_params)?;
                let feature_id: crate::contract::MapFeatureId =
                    feature.parse().map_err(invalid_params)?;
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .feature_revision(&identity, &scope, &layer_id, &feature_id, revision)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("feature revision"))?,
                );
            }
            if let Some((layer, version)) = uris::parse_feature_schema(uri) {
                let layer_id: crate::contract::FeatureLayerId =
                    layer.parse().map_err(invalid_params)?;
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .schema_revision(&identity, &scope, &layer_id, version)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("feature schema revision"))?,
                );
            }
            if let Some((layer, version)) = uris::parse_feature_style(uri) {
                let layer_id: crate::contract::FeatureLayerId =
                    layer.parse().map_err(invalid_params)?;
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .style_revision(&identity, &scope, &layer_id, version)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("feature style revision"))?,
                );
            }
            if let Some((layer, feature)) = uris::parse_feature(uri) {
                let layer_id: crate::contract::FeatureLayerId =
                    layer.parse().map_err(invalid_params)?;
                let feature_id: crate::contract::MapFeatureId =
                    feature.parse().map_err(invalid_params)?;
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .feature(&identity, &scope, &layer_id, &feature_id)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("feature"))?,
                );
            }
            if let Some((layer, changeset)) = uris::parse_changeset(uri) {
                let layer_id: crate::contract::FeatureLayerId =
                    layer.parse().map_err(invalid_params)?;
                let changeset_id: crate::contract::FeatureChangeSetId =
                    changeset.parse().map_err(invalid_params)?;
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .changeset(&identity, &scope, &layer_id, &changeset_id)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("feature changeset"))?,
                );
            }
            if let Some((layer, publication)) = uris::parse_publication(uri) {
                let layer_id: crate::contract::FeatureLayerId =
                    layer.parse().map_err(invalid_params)?;
                let publication_id: crate::contract::LayerPublicationId =
                    publication.parse().map_err(invalid_params)?;
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .publication(&identity, &scope, &layer_id, &publication_id)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("layer publication"))?,
                );
            }
            if let Some((layer, publication, product)) = uris::parse_layer_product(uri) {
                let layer_id: crate::contract::FeatureLayerId =
                    layer.parse().map_err(invalid_params)?;
                let publication_id: crate::contract::LayerPublicationId =
                    publication.parse().map_err(invalid_params)?;
                let product_id: crate::contract::LayerProductId =
                    product.parse().map_err(invalid_params)?;
                let product = self
                    .state
                    .authoring
                    .layer_product(&identity, &scope, &product_id)
                    .await
                    .map_err(internal)?
                    .filter(|product| {
                        product.layer_id == layer_id && product.publication_id == publication_id
                    })
                    .ok_or_else(|| not_found("map layer product"))?;
                return json_resource(uri, &product);
            }
            if let Some(layer) = uris::parse_feature_layer(uri) {
                let layer_id: crate::contract::FeatureLayerId =
                    layer.parse().map_err(invalid_params)?;
                return json_resource(
                    uri,
                    &self
                        .state
                        .authoring
                        .layer(&identity, &scope, &layer_id)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("feature layer"))?,
                );
            }
        }
        if let Some(artifact_id) = uris::parse_artifact(uri) {
            require_any_scope(&context, &["map:dataset:read", "map:feature:read"])?;
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
        let identity = require_scope(&context, "map:dataset:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
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
        let identity = if is_feature_template(&reference.uri) {
            require_scope(&context, "map:feature:read")?
        } else {
            require_scope(&context, "map:dataset:read")?
        };
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let values = completion_values(
            &self.state,
            &identity,
            &scope,
            &reference.uri,
            &request.argument.name,
        )
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
        if !is_subscribable(&request.uri) {
            return Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ));
        }
        let identity = if is_feature_subscribable(&request.uri) {
            require_scope(&context, "map:feature:read")?
        } else {
            require_scope(&context, "map:dataset:read")?
        };
        self.state
            .subscriptions
            .subscribe(request.uri, identity.actor.id, context.peer.clone())
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        if !is_subscribable(&request.uri) {
            return Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ));
        }
        let identity = if is_feature_subscribable(&request.uri) {
            require_scope(&context, "map:feature:read")?
        } else {
            require_scope(&context, "map:dataset:read")?
        };
        self.state
            .subscriptions
            .unsubscribe(&request.uri, &identity.actor.id)
            .await;
        Ok(())
    }
}

/// Every advertised resource template. `list_resource_templates` serves this
/// list and the `map://contract` capability inventory declares it, so the two
/// cannot diverge.
fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(uris::DOC_TEMPLATE, "Server document")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        template(
            uris::SOURCE_TEMPLATE,
            "Map source",
            "Authorized source provenance.",
        ),
        template(
            uris::ACQUISITION_TEMPLATE,
            "Map acquisition",
            "Governed acquisition job (map:admin).",
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
        template(
            uris::FEATURE_LAYER_TEMPLATE,
            "Authored feature layer",
            "Work Context-scoped layer head with pinned schema and style revisions.",
        ),
        template(
            uris::FEATURE_SCHEMA_TEMPLATE,
            "Feature schema revision",
            "Immutable JSON Schema 2020-12 property contract.",
        ),
        template(
            uris::FEATURE_STYLE_TEMPLATE,
            "Feature style revision",
            "Immutable safe map style revision.",
        ),
        template(
            uris::FEATURES_TEMPLATE,
            "Authored feature query",
            "Paginated current or published GeoJSON features with spatial, temporal, and CQL2 filters.",
        ),
        template(
            uris::FEATURE_TEMPLATE,
            "Authored feature",
            "Current canonical feature head.",
        ),
        template(
            uris::FEATURE_REVISION_TEMPLATE,
            "Authored feature revision",
            "Immutable canonical feature revision.",
        ),
        template(
            uris::CHANGESET_TEMPLATE,
            "Feature changeset",
            "Atomic authored feature commit.",
        ),
        template(
            uris::PUBLICATION_TEMPLATE,
            "Feature layer publication",
            "Immutable published layer revision.",
        ),
        template(
            uris::LAYER_PRODUCT_TEMPLATE,
            "Feature layer product",
            "Immutable artifact derived from a published layer revision.",
        ),
        template(
            uris::COMPOSITION_TEMPLATE,
            "Map composition",
            "Mutable head of a governed publication-pinned map composition.",
        ),
        template(
            uris::COMPOSITION_REVISION_TEMPLATE,
            "Map composition revision",
            "Immutable map composition revision.",
        ),
    ]
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
    let memberships = identity.actor.group_memberships();
    Ok(PlaneCaller {
        identity,
        memberships,
        bearer_token,
    })
}

fn identity_has_scope(identity: &GatewayInternalIdentity, required: &str) -> bool {
    identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
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
    required: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    if !identity_has_scope(&identity, required) {
        return Err(McpError::invalid_request(
            format!("scope `{required}` is required"),
            None,
        ));
    }
    Ok(identity)
}

fn require_any_scope(
    context: &RequestContext<RoleServer>,
    required: &[&str],
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    if !required
        .iter()
        .any(|required| identity_has_scope(&identity, required))
    {
        return Err(McpError::invalid_request(
            format!("one of scopes [{}] is required", required.join(", ")),
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
fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![json_resource_descriptor(
        uris::DOCS_URI.to_owned(),
        "Server documents".to_owned(),
        "Index of the crate documents embedded at build time.",
    )];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(uris::doc_uri(doc.id), doc.title)
                .with_title(doc.title)
                .with_description("Crate document embedded at build time.")
                .with_mime_type("text/markdown"),
        );
    }
    resources.push(json_resource_descriptor(
        uris::CONTRACT_URI.to_owned(),
        "Contract declaration".to_owned(),
        "Machine-readable contract revision, compliance, and capability inventory.",
    ));
    resources
}

/// Stable resource URIs declared in the `map://contract` capability
/// inventory: the well-known surface, the app views, and the index resources
/// `list_resources` always serves. Per-entity resources are enumerated per
/// identity at list time and are covered by the resource templates instead.
/// Constructed beside `list_resources`; when a stable resource is added or
/// removed there, update this list in the same change so the served
/// declaration cannot silently diverge.
fn stable_resource_uris() -> Vec<String> {
    let mut resource_uris: Vec<String> = well_known_resources()
        .into_iter()
        .map(|resource| resource.uri.clone())
        .collect();
    resource_uris.extend(
        [
            uris::ADMIN_APP_URI,
            uris::EDITOR_APP_URI,
            uris::ACQUISITIONS_URI,
            uris::ACTIVE_RELEASES_URI,
            uris::FEATURE_LAYERS_URI,
            uris::PUBLICATIONS_URI,
            uris::LAYER_PRODUCTS_URI,
            uris::COMPOSITIONS_URI,
        ]
        .map(str::to_owned),
    );
    resource_uris.extend(
        root_resources()
            .into_iter()
            .map(|resource| resource.uri.clone()),
    );
    resource_uris.sort();
    resource_uris
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
    identity: &GatewayInternalIdentity,
    scope: &crate::catalog::MapScope,
    template: &str,
    argument: &str,
) -> anyhow::Result<Vec<String>> {
    let values = match (template, argument) {
        (uris::DOC_TEMPLATE, "doc_id") => SERVER_DOCS.iter().map(|doc| doc.id.to_owned()).collect(),
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
        (
            uris::FEATURE_LAYER_TEMPLATE
            | uris::FEATURE_SCHEMA_TEMPLATE
            | uris::FEATURE_STYLE_TEMPLATE
            | uris::FEATURES_TEMPLATE
            | uris::FEATURE_TEMPLATE
            | uris::FEATURE_REVISION_TEMPLATE
            | uris::CHANGESET_TEMPLATE
            | uris::PUBLICATION_TEMPLATE
            | uris::LAYER_PRODUCT_TEMPLATE,
            "layer_id",
        ) => state
            .authoring
            .list_layers(identity, scope, true)
            .await?
            .into_iter()
            .map(|value| value.layer_id.to_string())
            .collect(),
        (uris::FEATURE_SCHEMA_TEMPLATE, "schema_version") => state
            .authoring
            .list_layers(identity, scope, true)
            .await?
            .into_iter()
            .map(|value| value.schema.version.to_string())
            .collect(),
        (uris::FEATURE_STYLE_TEMPLATE, "style_version") => state
            .authoring
            .list_layers(identity, scope, true)
            .await?
            .into_iter()
            .filter_map(|value| value.style.map(|style| style.version.to_string()))
            .collect(),
        (
            uris::PUBLICATION_TEMPLATE | uris::FEATURES_TEMPLATE | uris::LAYER_PRODUCT_TEMPLATE,
            "publication_id",
        ) => state
            .authoring
            .list_publications(identity, scope, None)
            .await?
            .into_iter()
            .map(|value| value.publication_id.to_string())
            .collect(),
        (uris::LAYER_PRODUCT_TEMPLATE, "product_id") => state
            .authoring
            .list_layer_products(identity, scope, None)
            .await?
            .into_iter()
            .map(|value| value.product_id.to_string())
            .collect(),
        (uris::COMPOSITION_TEMPLATE | uris::COMPOSITION_REVISION_TEMPLATE, "composition_id") => {
            state
                .authoring
                .list_compositions(identity, scope, true)
                .await?
                .into_iter()
                .map(|value| value.composition_id.to_string())
                .collect()
        }
        (uris::COMPOSITION_REVISION_TEMPLATE, "composition_revision") => state
            .authoring
            .list_compositions(identity, scope, true)
            .await?
            .into_iter()
            .map(|value| value.current.revision.to_string())
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
    uri.starts_with("map://feature-layer/") || uri.starts_with("map://composition/")
}

#[cfg(test)]
mod well_known_tests {
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, ContractDeclaration, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::{MapMcp, SERVER_DOCS, resource_templates, stable_resource_uris};
    use crate::uris;

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "map");
        let agents = SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
        assert!(agents.body.contains("## Contract Compliance"));
        let design = SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
        assert!(!design.body.is_empty());
        let index = SERVER_DOCS.llms_txt();
        assert!(index.contains("(docs/agents)"));
        assert!(index.contains("(docs/design)"));
    }

    #[test]
    fn contract_declaration_resolves_from_the_embedded_manual() {
        let declaration =
            ContractDeclaration::from_docs(&SERVER_DOCS, MapMcp::capability_inventory());
        assert_eq!(declaration.server, "map");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        let c17 = declaration
            .compliance
            .iter()
            .find(|item| item.id == "C17")
            .expect("C17 declared");
        assert_eq!(c17.status, ComplianceStatus::Pending);
        assert!(c17.note.is_some(), "pending items must state a reason");
        let json = serde_json::to_value(&declaration).expect("declaration serializes");
        assert_eq!(json["server"], "map");
    }

    #[test]
    fn capability_inventory_matches_the_registered_surface() {
        let inventory = MapMcp::capability_inventory();
        for tool in ["search_locations", "route", "commit_feature_changes"] {
            assert!(
                inventory.tools.iter().any(|name| name == tool),
                "inventory is missing tool {tool}"
            );
        }
        for uri in [
            uris::DOCS_URI,
            uris::CONTRACT_URI,
            uris::ADMIN_APP_URI,
            uris::EDITOR_APP_URI,
            uris::DATASETS_URI,
        ] {
            assert!(
                inventory.resources.contains(&uri.to_owned()),
                "inventory is missing resource {uri}"
            );
        }
        assert!(inventory.resources.contains(&uris::doc_uri("agents")));
        assert_eq!(
            stable_resource_uris().len(),
            inventory.resources.len(),
            "inventory resources come from stable_resource_uris"
        );
        assert!(
            inventory
                .resource_templates
                .contains(&uris::DOC_TEMPLATE.to_owned())
        );
        assert_eq!(
            resource_templates().len(),
            inventory.resource_templates.len()
        );
        assert!(
            inventory
                .prompts
                .iter()
                .any(|name| name == "prepare_route_request")
        );
        assert_eq!(
            inventory.tasks,
            [
                "route",
                "route_matrix",
                "reachable_area",
                "import_feature_layer",
                "export_feature_layer",
                "build_vector_tiles",
            ]
        );
        for task in &inventory.tasks {
            assert!(
                inventory.tools.contains(task),
                "task-augmented tool {task} must also be a registered tool"
            );
        }
    }
}

#[cfg(test)]
mod admin_app_tests {
    use super::MapMcp;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!MapMcp::tool_router().list_all().is_empty());
    }

    const ADMIN_APP: &str = include_str!("../assets/admin-app.html");
    const EDITOR_APP: &str = include_str!("../assets/editor-app.html");

    #[test]
    fn acquisition_idempotency_uses_the_browser_uuid_generator() {
        assert!(ADMIN_APP.contains("return crypto.randomUUID();"));
        assert!(!ADMIN_APP.contains("return idempotencyKey();"));
    }

    #[test]
    fn acquisition_submit_owns_validation_progress_and_tool_errors() {
        assert!(ADMIN_APP.contains(r#"id="acquire-form" novalidate"#));
        assert!(ADMIN_APP.contains("Enter west, south, east, and north coverage bounds"));
        assert!(ADMIN_APP.contains(r#"submit.textContent = "Starting…";"#));
        assert!(ADMIN_APP.contains("result.isError || result.is_error"));
    }

    #[test]
    fn editor_uses_only_canonical_mcp_resources_and_tools() {
        assert!(EDITOR_APP.contains("resources/read"));
        assert!(EDITOR_APP.contains("tools/call"));
        for tool in [
            "create_feature_layer",
            "validate_feature_changes",
            "commit_feature_changes",
            "query_features",
            "publish_feature_layer",
            "create_map_composition",
        ] {
            assert!(EDITOR_APP.contains(tool), "editor is missing {tool}");
        }
        assert!(!EDITOR_APP.contains("fetch("));
        assert!(!EDITOR_APP.contains("XMLHttpRequest"));
        assert!(!EDITOR_APP.contains("innerHTML"));
    }
}
