use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, LazyLock};

use crate::contract::{LiveSessionId, UavGrantCursor, UavScope};
use chrono::Utc;
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
use serde_json::json;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, Page, SubscriptionHub, UsageKind, UsageRecord, UsageReport,
    docs::ServerDocs, paginate,
};
use veoveo_task_runtime::{TaskRetentionPin, TaskSnapshot, TaskStatus};

use crate::contract::{
    CameraCodec, CameraEncoder, CameraLifecycle, CameraState, CaptureDatasetRequest,
    CloseLiveViewRequest, CommandAcknowledgement, ConfigureWorldOutput, ConfigureWorldRequest,
    DurableOperation, ExecuteVehicleMissionPlanRequest, GrantVehicleControlRequest,
    OpenLiveViewRequest, PrepareVehicleMissionRequest, RenewLiveViewRequest,
    RevokeVehicleControlRequest, RunScenarioRequest, SessionId, SessionRequest, SimulationCommand,
    SimulationLifecycle, SimulationState, StepSimulationRequest, TakeoffRequest, TileLifecycle,
    TileState, VehicleControlPermission, VehicleId, VehicleRequest, VehicleState, Wgs84Position,
};
use crate::uris;

use super::auth::{identity_has_scope, require_any_scope as require_any_identity_scope};
use super::control_authority::ControlAuthorityError;
use super::index;
#[path = "bootstrap.rs"]
mod bootstrap;
#[path = "resources.rs"]
pub(super) mod resources;
use super::live_view::LiveViewError;
use super::ownership::{internal_caller, internal_identity};
use super::prompts::UavSimPrompt;
use super::state::AppState;
use super::task_extension::UavSimTaskExtension;
use super::task_worker::{await_result, start_operation, start_vehicle_mission_plan};
pub(super) use bootstrap::serve;
use resources::resource_templates;

const SERVER_SLUG: &str = "uav-sim";
const LIST_PAGE_SIZE: usize = 100;
const LIVE_APP_TOOLS: &[&str] = &[
    "list_live_cameras",
    "open_live_view",
    "renew_live_view",
    "close_live_view",
];
/// The crate documents embedded at build time and served under the well-known
/// surface: `uav-sim://docs`, `uav-sim://docs/{doc_id}`, `uav-sim://contract`,
/// and the administrative `admin/docs` routes (contract C18-C21).
pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!(SERVER_SLUG));
#[derive(Clone)]
pub(super) struct UavSimMcp {
    state: Arc<AppState>,
    task_service: UavSimTaskExtension,
    #[allow(dead_code)]
    tool_router: ToolRouter<UavSimMcp>,
}

impl UavSimMcp {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self {
            task_service: UavSimTaskExtension::new(state.clone()),
            state,
            tool_router: Self::tool_router(),
        }
    }

    async fn current_state(&self) -> Result<SimulationState, McpError> {
        let mut state = self.state.adapter.state().await.map_err(internal)?;
        self.state
            .live_views
            .project_product_usage(&mut state)
            .await;
        Ok(state)
    }

    async fn state_for(&self, session_id: &SessionId) -> Result<SimulationState, McpError> {
        let state = self.current_state().await?;
        if &state.session_id == session_id {
            Ok(state)
        } else {
            Err(McpError::resource_not_found(
                "simulation session not found",
                None,
            ))
        }
    }

    async fn visible_state(
        &self,
        identity: &GatewayInternalIdentity,
    ) -> Result<SimulationState, McpError> {
        let mut state = self.current_state().await?;
        if identity_has_scope(identity, UavScope::Read)
            || identity_has_scope(identity, UavScope::Admin)
        {
            return Ok(state);
        }
        super::auth::require_scope(identity, UavScope::Control)?;
        let visible_vehicle_ids = self
            .state
            .control_authority
            .inspectable_vehicles(
                identity,
                &state.session_id,
                &state
                    .vehicles
                    .iter()
                    .map(|vehicle| vehicle.vehicle_id.clone())
                    .collect::<Vec<_>>(),
            )
            .await
            .map_err(authority_error)?;
        state
            .vehicles
            .retain(|vehicle| visible_vehicle_ids.contains(&vehicle.vehicle_id));
        state
            .cameras
            .retain(|camera| visible_vehicle_ids.contains(&camera.vehicle_id));
        Ok(state)
    }

    async fn apply_command(&self, command: SimulationCommand) -> Result<CallToolResult, McpError> {
        let result = self
            .state
            .adapter
            .command(&command)
            .await
            .map_err(invalid)?;
        self.state
            .subscribers
            .notify_resource_updated(result.resource_uri.clone())
            .await;
        let session_id = command_session(&command);
        self.state
            .subscribers
            .notify_resource_updated(uris::session(session_id))
            .await;
        structured_result(result.detail.clone(), &result)
    }

    async fn require_vehicle_permission(
        &self,
        context: &RequestContext<RoleServer>,
        session_id: &SessionId,
        vehicle_id: &VehicleId,
        permission: VehicleControlPermission,
    ) -> Result<GatewayInternalIdentity, McpError> {
        let identity = require_scope(context, UavScope::Control)?;
        let state = self.state_for(session_id).await?;
        if !state
            .vehicles
            .iter()
            .any(|vehicle| &vehicle.vehicle_id == vehicle_id)
        {
            return Err(McpError::resource_not_found(
                format!("Vehicle `{}` was not found in this session.", vehicle_id),
                None,
            ));
        }
        self.state
            .control_authority
            .require_permission(&identity, session_id, vehicle_id, permission)
            .await
            .map_err(authority_error)?;
        Ok(identity)
    }

    async fn start_and_wait(
        &self,
        context: &RequestContext<RoleServer>,
        operation: DurableOperation,
    ) -> Result<CallToolResult, McpError> {
        let snapshot = start_operation(
            self.state.clone(),
            internal_caller(context)?,
            operation,
            BTreeSet::<TaskRetentionPin>::new(),
        )
        .await
        .map_err(|error| McpError::internal_error(error, None))?;
        await_result(&self.state, &snapshot.task_id.to_string()).await
    }
}

#[tool_router]
impl UavSimMcp {
    #[tool(
        title = "Configure UAV frame world",
        description = "Bind a new simulation session to one Frames world revision and one simulation frame. Do this once, before flying.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ConfigureWorldOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn configure_world(
        &self,
        Parameters(request): Parameters<ConfigureWorldRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, UavScope::Admin)?;
        let output = self
            .state
            .adapter
            .configure_world(&request)
            .await
            .map_err(invalid)?;
        self.state
            .subscribers
            .notify_resource_updated(uris::session(&request.session_id))
            .await;
        self.state
            .subscribers
            .notify_resource_updated(uris::world(&request.session_id))
            .await;
        structured_result("configured immutable frame world".to_owned(), &output)
    }

    #[tool(
        title = "Get UAV simulation state",
        description = "Read the session's current state: Google Photorealistic 3D Tiles loading, sensor-stream health, recording, and vehicles.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SimulationState>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn get_simulation_state(
        &self,
        Parameters(request): Parameters<SessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_any_scope(
            &context,
            &[UavScope::Read, UavScope::Control, UavScope::Admin],
        )?;
        let state = self.visible_state(&identity).await?;
        require_session(&state, &request.session_id)?;
        structured_result("current UAV simulation state".to_owned(), &state)
    }

    #[tool(
        title = "List active vehicle control grants",
        description = "Read up to 100 active vehicle grants in one session. Pass next_cursor back as cursor until it is null to traverse all grants. Each grant gives the vehicle id, the permissions, and the Map mobility-profile URI to use in Map route requests. Only a grant gives you control of a vehicle; naming a vehicle id in your input does not.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::CollectionPage<crate::contract::VehicleControlGrant>>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn list_active_vehicle_control_grants(
        &self,
        Parameters(request): Parameters<crate::contract::ActiveVehicleGrantsRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_any_scope(&context, &[UavScope::Control, UavScope::Admin])?;
        self.state_for(&request.session_id).await?;
        let include_all = identity_has_scope(&identity, UavScope::Admin);
        let after = request
            .cursor
            .as_deref()
            .map(|cursor| UavGrantCursor::parse(Some(&request.session_id), cursor))
            .transpose()
            .map_err(invalid)?;
        let grants = self
            .state
            .control_authority
            .grants_page(
                &identity,
                include_all,
                Some(&request.session_id),
                after.as_ref().map(UavGrantCursor::position),
            )
            .await
            .map_err(authority_error)?;
        structured_result(
            format!("{} active vehicle control grant(s)", grants.items.len()),
            &grants,
        )
    }

    #[tool(
        title = "Grant vehicle control",
        description = "Grant one user or service control of one simulated vehicle, with explicit UAV permissions and one Map mobility profile.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::VehicleControlGrant>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn grant_vehicle_control(
        &self,
        Parameters(request): Parameters<GrantVehicleControlRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, UavScope::Admin)?;
        let state = self.state_for(&request.session_id).await?;
        if !state
            .vehicles
            .iter()
            .any(|vehicle| vehicle.vehicle_id == request.vehicle_id)
        {
            return Err(McpError::resource_not_found(
                format!(
                    "Vehicle `{}` was not found in this session.",
                    request.vehicle_id
                ),
                None,
            ));
        }
        let grant = self
            .state
            .control_authority
            .grant(&identity, request)
            .await
            .map_err(authority_error)?;
        self.state
            .subscribers
            .notify_resource_updated(uris::CONTROL_GRANTS)
            .await;
        self.state
            .subscribers
            .notify_resource_updated(uris::control_grant(&grant.grant_id))
            .await;
        structured_result(
            format!("granted vehicle control {}", grant.grant_id),
            &grant,
        )
    }

    #[tool(
        title = "Revoke vehicle control",
        description = "Revoke one vehicle grant. Pass the grant revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::VehicleControlGrant>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn revoke_vehicle_control(
        &self,
        Parameters(request): Parameters<RevokeVehicleControlRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, UavScope::Admin)?;
        let grant = self
            .state
            .control_authority
            .revoke(&identity, request)
            .await
            .map_err(authority_error)?;
        self.state
            .subscribers
            .notify_resource_updated(uris::CONTROL_GRANTS)
            .await;
        self.state
            .subscribers
            .notify_resource_updated(uris::control_grant(&grant.grant_id))
            .await;
        structured_result(
            format!("revoked vehicle control {}", grant.grant_id),
            &grant,
        )
    }

    #[tool(
        title = "Prepare vehicle mission",
        description = "Check a Map route handoff against your vehicle grant and the session's Frames world, and return a mission plan to pass to `execute_vehicle_mission_plan`.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::VehicleMissionPlan>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn prepare_vehicle_mission(
        &self,
        Parameters(request): Parameters<PrepareVehicleMissionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, UavScope::Control)?;
        let state = self.state_for(&request.session_id).await?;
        if !state
            .vehicles
            .iter()
            .any(|vehicle| vehicle.vehicle_id == request.vehicle_id)
        {
            return Err(McpError::resource_not_found(
                format!(
                    "Vehicle `{}` was not found in this session.",
                    request.vehicle_id
                ),
                None,
            ));
        }
        let Some(world) = state.world else {
            return Err(McpError::invalid_request(
                "simulation world is not configured",
                None,
            ));
        };
        if request.expected_world_revision_uri != world.revision_uri {
            return Err(McpError::invalid_params(
                "mission expected_world_revision_uri does not match the session",
                None,
            ));
        }
        let plan = self
            .state
            .control_authority
            .prepare_plan(&identity, request)
            .await
            .map_err(authority_error)?;
        self.state
            .subscribers
            .notify_resource_updated(uris::MISSION_PLANS)
            .await;
        self.state
            .subscribers
            .notify_resource_updated(uris::mission_plan(&plan.plan_id))
            .await;
        structured_result(format!("prepared vehicle mission {}", plan.plan_id), &plan)
    }

    #[tool(
        title = "List authoritative UAV live cameras",
        description = "List the operator cameras the simulator renders. Each camera is rendered and encoded once and shared by every authorized viewer.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Vec<crate::contract::LiveCameraDescriptor>>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn list_live_cameras(
        &self,
        Parameters(request): Parameters<SessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, UavScope::Stream)?;
        let state = self.state_for(&request.session_id).await?;
        structured_result(
            "authoritative UAV live cameras".to_owned(),
            &state.live_cameras,
        )
    }

    #[tool(
        title = "Open authoritative UAV live view",
        description = "Authorize this browser to watch an existing simulator camera. This does not start another render or encode.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::LiveViewConnection>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true)
    )]
    async fn open_live_view(
        &self,
        Parameters(request): Parameters<OpenLiveViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, UavScope::Stream)?;
        let owner = crate::server::ownership::live_view_owner(&identity);
        let details = live_view_details(&request.session_id, Some(request.camera_id.as_str()));
        let result = self
            .state
            .live_views
            .open(owner, identity.actor.id.clone(), request)
            .await;
        let connection = match result {
            Ok(connection) => connection,
            Err(error) => {
                let mut denied_details = details;
                denied_details.extend(error.audit_details());
                denied_details.insert(
                    "failure_code".to_owned(),
                    serde_json::Value::String(error.code().to_owned()),
                );
                audit_live_view(
                    &self.state,
                    &identity,
                    None,
                    "open_denied",
                    veoveo_platform_store::AuditOutcome::Denied,
                    denied_details,
                )
                .await;
                return Err(live_view_error(error));
            }
        };
        audit_live_view(
            &self.state,
            &identity,
            Some(&connection.stream.live_view_id),
            "opened",
            veoveo_platform_store::AuditOutcome::Allowed,
            details,
        )
        .await;
        self.state
            .subscribers
            .notify_resource_updated(connection.stream.resource_uri.as_str())
            .await;
        self.state
            .subscribers
            .notify_resource_updated(uris::live_views(&connection.stream.session_id))
            .await;
        structured_result(
            format!("opened {}", connection.stream.resource_uri.as_str()),
            &connection,
        )
    }

    #[tool(
        title = "Renew authoritative UAV live view",
        description = "Renew this browser's camera authorization and rotate its stream token.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::LiveViewConnection>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true)
    )]
    async fn renew_live_view(
        &self,
        Parameters(request): Parameters<RenewLiveViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, UavScope::Stream)?;
        let owner = crate::server::ownership::live_view_owner(&identity);
        let session_id = request.session_id.clone();
        let live_view_id = request.live_view_id.clone();
        let result = self
            .state
            .live_views
            .renew(&owner, &identity.actor.id, request)
            .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let action = if matches!(error, LiveViewError::AuthorityRevoked) {
                    "viewer_authority_revoked"
                } else {
                    "renew_denied"
                };
                let mut details = live_view_details(&session_id, None);
                details.extend(error.audit_details());
                details.insert(
                    "failure_code".to_owned(),
                    serde_json::Value::String(error.code().to_owned()),
                );
                audit_live_view(
                    &self.state,
                    &identity,
                    Some(&live_view_id),
                    action,
                    veoveo_platform_store::AuditOutcome::Denied,
                    details,
                )
                .await;
                if matches!(error, LiveViewError::AuthorityRevoked) {
                    self.state
                        .subscribers
                        .notify_resource_updated(uris::live_view(&session_id, &live_view_id))
                        .await;
                    self.state
                        .subscribers
                        .notify_resource_updated(uris::live_views(&session_id))
                        .await;
                }
                return Err(live_view_error(error));
            }
        };
        audit_live_view(
            &self.state,
            &identity,
            Some(&live_view_id),
            "renewed",
            veoveo_platform_store::AuditOutcome::Allowed,
            live_view_details(
                &result.stream.session_id,
                Some(result.stream.camera_id.as_str()),
            ),
        )
        .await;
        self.state
            .subscribers
            .notify_resource_updated(result.stream.resource_uri.as_str())
            .await;
        structured_result(
            format!("renewed {}", result.stream.resource_uri.as_str()),
            &result,
        )
    }

    #[tool(
        title = "Close authoritative UAV live view",
        description = "Revoke this browser's camera authorization. The camera and other viewers keep running.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::CloseLiveViewResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn close_live_view(
        &self,
        Parameters(request): Parameters<CloseLiveViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, UavScope::Stream)?;
        let owner = crate::server::ownership::live_view_owner(&identity);
        let session_id = request.session_id.clone();
        let live_view_id = request.live_view_id.clone();
        let result = self
            .state
            .live_views
            .close(&owner, &identity.actor.id, request)
            .await;
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                let mut details = live_view_details(&session_id, None);
                details.extend(error.audit_details());
                details.insert(
                    "failure_code".to_owned(),
                    serde_json::Value::String(error.code().to_owned()),
                );
                audit_live_view(
                    &self.state,
                    &identity,
                    Some(&live_view_id),
                    "close_denied",
                    veoveo_platform_store::AuditOutcome::Denied,
                    details,
                )
                .await;
                return Err(live_view_error(error));
            }
        };
        audit_live_view(
            &self.state,
            &identity,
            Some(&live_view_id),
            "closed",
            veoveo_platform_store::AuditOutcome::Allowed,
            live_view_details(&session_id, None),
        )
        .await;
        self.state
            .subscribers
            .notify_resource_updated(&result.resource_uri)
            .await;
        self.state
            .subscribers
            .notify_resource_updated(uris::live_views(&session_id))
            .await;
        structured_result("closed authoritative UAV live view".to_owned(), &result)
    }

    #[tool(
        title = "Pause UAV simulation",
        description = "Pause one running simulation session.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommandAcknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn pause_simulation(
        &self,
        Parameters(request): Parameters<SessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, UavScope::Admin)?;
        self.apply_command(SimulationCommand::Pause(request)).await
    }

    #[tool(
        title = "Resume UAV simulation",
        description = "Resume one paused simulation session.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommandAcknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn resume_simulation(
        &self,
        Parameters(request): Parameters<SessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, UavScope::Admin)?;
        self.apply_command(SimulationCommand::Resume(request)).await
    }

    #[tool(
        title = "Reset UAV simulation",
        description = "Reset the stage and vehicles to the declared scenario start.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommandAcknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn reset_simulation(
        &self,
        Parameters(request): Parameters<SessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, UavScope::Admin)?;
        self.apply_command(SimulationCommand::Reset(request)).await
    }

    #[tool(
        title = "Step UAV simulation",
        description = "Advance a paused session by a number of physics steps.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommandAcknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn step_simulation(
        &self,
        Parameters(request): Parameters<StepSimulationRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, UavScope::Admin)?;
        self.apply_command(SimulationCommand::Step(request)).await
    }

    #[tool(
        title = "Arm simulated UAV",
        description = "Arm one PX4-backed vehicle after simulator safety checks.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommandAcknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn arm_vehicle(
        &self,
        Parameters(request): Parameters<VehicleRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.require_vehicle_permission(
            &context,
            &request.session_id,
            &request.vehicle_id,
            VehicleControlPermission::Execute,
        )
        .await?;
        self.apply_command(SimulationCommand::Arm(request)).await
    }

    #[tool(
        title = "Take off simulated UAV",
        description = "Arm one PX4 vehicle and start a takeoff to a relative altitude, in one step.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommandAcknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn takeoff_vehicle(
        &self,
        Parameters(request): Parameters<TakeoffRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.require_vehicle_permission(
            &context,
            &request.session_id,
            &request.vehicle_id,
            VehicleControlPermission::Execute,
        )
        .await?;
        self.apply_command(SimulationCommand::Takeoff(request))
            .await
    }

    #[tool(
        title = "Land simulated UAV",
        description = "Command one PX4-backed vehicle to land.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommandAcknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn land_vehicle(
        &self,
        Parameters(request): Parameters<VehicleRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.require_vehicle_permission(
            &context,
            &request.session_id,
            &request.vehicle_id,
            VehicleControlPermission::Abort,
        )
        .await?;
        self.apply_command(SimulationCommand::Land(request)).await
    }

    #[tool(
        title = "Run UAV scenario",
        description = "Run a time-limited live scenario in the loaded Google Photorealistic 3D Tiles world. Run as an MCP Task; an interrupted run is not retried.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::ScenarioResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn run_scenario(
        &self,
        Parameters(request): Parameters<RunScenarioRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, UavScope::Admin)?;
        self.start_and_wait(&context, DurableOperation::RunScenario(request))
            .await
    }

    #[tool(
        title = "Execute vehicle mission plan",
        description = "Fly a mission plan from `prepare_vehicle_mission` with one vehicle. The call takes your exclusive command lease for that vehicle while it flies.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::MissionResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn execute_vehicle_mission_plan(
        &self,
        Parameters(request): Parameters<ExecuteVehicleMissionPlanRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, UavScope::Control)?;
        let snapshot = start_vehicle_mission_plan(
            self.state.clone(),
            internal_caller(&context)?,
            request,
            BTreeSet::<TaskRetentionPin>::new(),
        )
        .await
        .map_err(|error| McpError::invalid_request(error, None))?;
        await_result(&self.state, &snapshot.task_id.to_string()).await
    }

    #[tool(
        title = "Capture UAV dataset",
        description = "Record sensor data for a time interval and return the recording identities. Run as an MCP Task; an interrupted capture is not retried.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::CaptureDatasetResult>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn capture_dataset(
        &self,
        Parameters(request): Parameters<CaptureDatasetRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, UavScope::Admin)?;
        self.start_and_wait(&context, DurableOperation::CaptureDataset(request))
            .await
    }
}

#[tool_handler]
impl ServerHandler for UavSimMcp {
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
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
        capabilities.extensions.get_or_insert_default().insert(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            rmcp::model::JsonObject::new(),
        );
        let mut info = ServerConfig::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new(SERVER_SLUG, env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Fly simulated UAVs. To fly a mission: (1) call `list_active_vehicle_control_grants` to find your vehicle and its Map mobility profile; (2) get a route handoff from Map MCP; (3) call `prepare_vehicle_mission` with that handoff; (4) call `execute_vehicle_mission_plan` as an MCP Task. Scenarios and sensor captures also run as MCP Tasks, and an interrupted flight is never retried automatically. Watch the operator cameras in the ui://uav-sim/live.html app."
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
        let tools = tools
            .into_iter()
            .map(|tool| {
                if LIVE_APP_TOOLS.contains(&tool.name.as_ref()) {
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        tool,
                        uris::LIVE_APP_URI,
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
        let resources = self.resource_descriptors(&context).await?;
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
        self.resource_read(request, context).await
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts: Vec<Prompt> = UavSimPrompt::ALL
            .into_iter()
            .map(UavSimPrompt::definition)
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
            UavSimPrompt::by_name(&request.name)
                .ok_or_else(|| McpError::invalid_params("unknown UAV simulation prompt", None))?
                .render(request.arguments)
        }
        .await
        .map(Into::into)
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
            self.require_subscribable(uri, &request_context).await?;
        }
        veoveo_task_runtime::listen_durable_subscriptions(
            &self.task_service,
            context,
            Some(self.state.subscribers.as_ref()),
            None,
        )
        .await
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        self.resource_complete(request, context).await
    }
}

pub(crate) fn fake_state() -> anyhow::Result<SimulationState> {
    let revision_uri = veoveo_frames_mcp::contract::FrameWorldRevisionUri::new(
        &veoveo_frames_mcp::contract::FrameWorldId::new("test-world")?,
        &veoveo_frames_mcp::contract::FrameWorldRevisionId::new("revision-1")?,
    );
    Ok(SimulationState {
        session_id: SessionId::new("session-alpha")?,
        lifecycle: SimulationLifecycle::Ready,
        simulation_time_s: 0.0,
        physics_step: 0,
        timing: crate::contract::RuntimeTimingState {
            physics_hz: 60,
            native_rendering_hz: 2,
            render_cycles: 0,
            physics_steps: 0,
            refresh_states_wall_seconds: 0.0,
            vehicle_update_wall_seconds: 0.0,
            state_update_wall_seconds: 0.0,
            dynamics_update_wall_seconds: 0.0,
            sensor_update_wall_seconds: 0.0,
            backend_state_wall_seconds: 0.0,
            flush_forces_wall_seconds: 0.0,
            after_step_wall_seconds: 0.0,
            native_update_wall_seconds: 0.0,
            render_cycle_wall_seconds: 0.0,
            maximum_physics_step_ms: 0.0,
            maximum_native_update_ms: 0.0,
            maximum_render_cycle_ms: 0.0,
        },
        world: Some(crate::contract::SimulationWorldBinding {
            revision_uri: revision_uri.clone(),
            spec_sha256: "a".repeat(64),
            simulation_frame_uri: veoveo_frames_mcp::contract::WorldFrameUri::new(
                &revision_uri,
                &veoveo_frames_mcp::contract::FrameId::new("isaac-world")?,
            ),
            georeference_origin: Wgs84Position {
                latitude_degrees: 13.6929,
                longitude_degrees: -89.2182,
                ellipsoid_height_m: 700.0,
            },
        }),
        tiles: TileState {
            lifecycle: TileLifecycle::Ready,
            source: "google_photorealistic_3d_tiles".to_owned(),
            ion_asset_id: 1,
            resident_tiles: 20,
            loading_tiles: 0,
            visible_tiles: 12,
            geometries_loaded: 20,
            geometries_rendered: 12,
            materials_loaded: 20,
            provider_generation: 1,
            event_sequence: 0,
            refresh_count: 0,
            last_failure: None,
            diagnostic: None,
        },
        cameras: vec![CameraState {
            vehicle_id: VehicleId::new("uav-1")?,
            entity_path: "/world/uav-sim/session-alpha/vehicle/uav-1/camera/down".to_owned(),
            lifecycle: CameraLifecycle::Ready,
            width: 640,
            height: 480,
            frame_rate_hz: 2,
            codec: CameraCodec::H264,
            encoder: CameraEncoder::NvidiaNvenc,
            transport: crate::contract::CameraTransport::RtspRtp,
            frames_observed: 10,
            last_access_unit_bytes: 32_768,
            last_frame_keyframe: false,
            render_pose: None,
            diagnostic: None,
        }],
        live_cameras: vec![crate::contract::LiveCameraDescriptor {
            camera_id: crate::contract::LiveCameraId::new("follow")?,
            session_id: LiveSessionId::new("session-alpha")?,
            revision: 1,
            rig: crate::contract::LiveCameraRig::FollowEntity {
                target_entity_id: crate::contract::LiveEntityId::new("uav-1")?,
                eye_offset_flu_m: crate::contract::LiveVector3 {
                    x: -8.0,
                    y: 0.0,
                    z: 3.0,
                },
                target_offset_flu_m: crate::contract::LiveVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.2,
                },
                smoothing: crate::contract::LiveCameraSmoothing {
                    translation_half_life_ms: 150,
                    rotation_half_life_ms: 120,
                    teleport_distance_millimetres: 100_000,
                    reset_after_gap_ms: 1_000,
                },
            },
            width_px: 1_280,
            height_px: 720,
            frame_rate_millihertz: 30_000,
            vertical_fov_degrees: 60.0,
            near_clip_m: 0.1,
            far_clip_m: 100_000.0,
            stream_policy: crate::contract::LiveCameraStreamPolicy::Continuous,
            health: crate::contract::LiveCameraHealth::Healthy,
            last_frame_at: Some(Utc::now()),
        }],
        stream_products: vec![crate::contract::LiveStreamProductState {
            stream_product_id: crate::contract::LiveStreamProductId::new("camera-atlas")?,
            camera_regions: vec![crate::contract::LiveCameraRegion {
                camera_id: crate::contract::LiveCameraId::new("follow")?,
                x_px: 0,
                y_px: 0,
                width_px: 1_280,
                height_px: 720,
            }],
            coded_width_px: 1_280,
            coded_height_px: 720,
            lifecycle: crate::contract::LiveStreamProductLifecycle::Ready,
            active_viewers: 0,
            connected_viewers: 0,
            nvenc_sessions: 1,
            encoded_frames: 0,
            source_to_render_p95_microseconds: None,
            source_to_render_samples: 0,
            last_frame_at: None,
            visible: None,
            diagnostic: None,
        }],
        vehicles: vec![VehicleState {
            vehicle_id: VehicleId::new("uav-1")?,
            flight_state: crate::contract::VehicleFlightState::Standby,
            wgs84: Wgs84Position {
                latitude_degrees: 13.6929,
                longitude_degrees: -89.2182,
                ellipsoid_height_m: 700.0,
            },
            enu: crate::contract::EnuVector {
                east_m: 0.0,
                north_m: 0.0,
                up_m: 0.0,
            },
            ned: crate::contract::NedVector {
                north_m: 0.0,
                east_m: 0.0,
                down_m: 0.0,
            },
            attitude_xyzw: crate::contract::QuaternionXyzw {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
            linear_velocity_enu_mps: crate::contract::EnuVector {
                east_m: 0.0,
                north_m: 0.0,
                up_m: 0.0,
            },
            battery_percent: 100.0,
            collision_count: 0,
            px4_connected: true,
        }],
        recordings: Vec::new(),
        updated_at: Utc::now(),
    })
}

fn command_session(command: &SimulationCommand) -> &SessionId {
    match command {
        SimulationCommand::Pause(request)
        | SimulationCommand::Resume(request)
        | SimulationCommand::Reset(request) => &request.session_id,
        SimulationCommand::Step(request) => &request.session_id,
        SimulationCommand::Arm(request) | SimulationCommand::Land(request) => &request.session_id,
        SimulationCommand::Takeoff(request) => &request.session_id,
    }
}

fn require_session(state: &SimulationState, session_id: &SessionId) -> Result<(), McpError> {
    if &state.session_id == session_id {
        Ok(())
    } else {
        Err(McpError::resource_not_found(
            "simulation session not found",
            None,
        ))
    }
}

fn require_live_session(state: &SimulationState, session: &LiveSessionId) -> Result<(), McpError> {
    let session = SessionId::new(session.as_str()).map_err(invalid)?;
    require_session(state, &session)
}

fn require_scope(
    context: &RequestContext<RoleServer>,
    required: UavScope,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    super::auth::require_scope(&identity, required)?;
    Ok(identity)
}

fn require_any_scope(
    context: &RequestContext<RoleServer>,
    required: &[UavScope],
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    require_any_identity_scope(&identity, required)?;
    Ok(identity)
}

fn authority_error(error: ControlAuthorityError) -> McpError {
    match error {
        ControlAuthorityError::Invalid(message) => McpError::invalid_params(message, None),
        ControlAuthorityError::Forbidden => McpError::invalid_request(
            "You don't have permission to control this vehicle. Check your grants with `list_active_vehicle_control_grants`.",
            None,
        ),
        ControlAuthorityError::NotFound => McpError::resource_not_found(error.to_string(), None),
        ControlAuthorityError::Conflict | ControlAuthorityError::VehicleBusy(_) => {
            McpError::invalid_request(error.to_string(), None)
        }
        ControlAuthorityError::Store(_)
        | ControlAuthorityError::Task(_)
        | ControlAuthorityError::Database(_)
        | ControlAuthorityError::Json(_)
        | ControlAuthorityError::Index(_) => McpError::internal_error(error.to_string(), None),
    }
}

fn live_view_details(
    session_id: &LiveSessionId,
    camera_id: Option<&str>,
) -> BTreeMap<String, serde_json::Value> {
    let mut details = BTreeMap::from([(
        "session_id".to_owned(),
        serde_json::Value::String(session_id.to_string()),
    )]);
    if let Some(camera_id) = camera_id {
        details.insert(
            "camera_id".to_owned(),
            serde_json::Value::String(camera_id.to_owned()),
        );
    }
    details
}

async fn audit_live_view(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    live_view_id: Option<&crate::contract::LiveViewId>,
    action: &'static str,
    outcome: veoveo_platform_store::AuditOutcome,
    details: BTreeMap<String, serde_json::Value>,
) {
    if let Err(error) = state
        .live_view_audit
        .append(identity, live_view_id, action, outcome, details)
        .await
    {
        tracing::error!(%error, action, "failed to persist live-view access audit");
    }
}

fn live_view_error(error: LiveViewError) -> McpError {
    match error {
        LiveViewError::SessionNotFound(_)
        | LiveViewError::CameraNotFound(_)
        | LiveViewError::ViewNotFound(_) => McpError::resource_not_found(error.to_string(), None),
        LiveViewError::Ownership | LiveViewError::AuthorityRevoked | LiveViewError::Access => {
            McpError::invalid_request("You don't have permission to view this camera.", None)
        }
        LiveViewError::CameraUnavailable | LiveViewError::ViewUnavailable => {
            McpError::invalid_request(error.to_string(), None)
        }
        _ => McpError::internal_error(error.to_string(), None),
    }
}

fn structured_result<T: Serialize>(message: String, value: &T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(message)]);
    result.structured_content = Some(serde_json::to_value(value).map_err(internal)?);
    Ok(result)
}

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE).map_err(invalid)
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

fn invalid(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        let tools = UavSimMcp::tool_router().list_all();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|tool| tool.name == "configure_world"));
        assert!(
            tools
                .iter()
                .any(|tool| tool.name == "list_active_vehicle_control_grants"),
            "UAV server must expose active caller-visible control authority to tool-only agents"
        );
        for owned in LIVE_APP_TOOLS {
            assert!(
                tools.iter().any(|tool| tool.name == *owned),
                "UAV server must own authoritative live-view tool {owned}"
            );
        }
        assert!(tools.iter().all(|tool| tool.name != "create_camera"));
    }

    #[test]
    fn fake_delivery_has_core_google_tiles_and_px4() {
        let state = fake_state().unwrap();
        assert_eq!(state.tiles.lifecycle, TileLifecycle::Ready);
        assert_eq!(state.tiles.source, "google_photorealistic_3d_tiles");
        assert!(
            state
                .cameras
                .iter()
                .all(|camera| camera.lifecycle == CameraLifecycle::Ready)
        );
        assert!(state.vehicles.iter().all(|vehicle| vehicle.px4_connected));
    }

    #[test]
    fn world_view_never_contains_a_credential() {
        let text = serde_json::to_string(&resources::world_view(&fake_state().unwrap())).unwrap();
        assert!(!text.contains("token"));
        assert!(!text.contains("CESIUM_ION_ACCESS_TOKEN"));
    }

    #[test]
    fn live_app_uses_the_default_complete_console_content_workspace() {
        let resource =
            resources::live_app_resource("wss://stream.example.com", &["uav-1-pilot".to_owned()]);
        let metadata = veoveo_mcp_apps_extension::resource_ui_meta(&resource)
            .expect("live App UI metadata is valid");
        assert_eq!(metadata.prefers_border, None);
        assert_eq!(
            metadata
                .csp
                .expect("live App CSP is present")
                .connect_domains,
            vec!["wss://stream.example.com"]
        );
        assert_eq!(
            veoveo_mcp_apps_extension::resource_agent_message_targets(&resource),
            ["uav-1-pilot"]
        );
        assert!(crate::live_app::html().contains("hardwareAcceleration:\"prefer-hardware\""));
        assert!(crate::live_app::html().contains("hardwareAcceleration:\"no-preference\""));
        assert!(!crate::live_app::html().contains("prefer-software"));
    }
}

#[cfg(test)]
mod well_known_tests {
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::SERVER_DOCS;

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "uav-sim");
        let agents = SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
        assert!(agents.body.contains("## Contract Compliance"));
        let design = SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
        assert!(!design.body.is_empty());
        let index = SERVER_DOCS.llms_txt();
        assert!(index.contains("(agents)"));
        assert!(index.contains("(design)"));
    }

    #[test]
    fn contract_declaration_resolves_from_the_embedded_manual() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        assert_eq!(declaration.server, "uav-sim");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        let json = serde_json::to_value(&declaration).expect("declaration serializes");
        assert_eq!(json["server"], "uav-sim");
    }

    #[test]
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
