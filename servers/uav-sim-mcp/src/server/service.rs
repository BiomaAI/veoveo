use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock};

use axum::{Router, extract::State, http::StatusCode, middleware, routing::get};
use chrono::Utc;
use clap::Parser;
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
        ResourceContents, ResourceTemplate, ServerCapabilities, ServerInfo, SubscriptionFilter,
        UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
    transport::streamable_http_server::StreamableHttpService,
};
use serde::Serialize;
use serde_json::json;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalIdentity, GatewayInternalTokenVerifier,
    GatewayInternalTrustBundle, LiveSessionId, LiveViewOwner, Page, ServerSlug, SubscriptionHub,
    TelemetryGuard, TokenIssuer, UsageKind, UsageRecord, UsageReport, docs::ServerDocs,
    init_server_telemetry, paginate, public_allowed_hosts,
};
use veoveo_task_runtime::{
    TaskRetentionPin, TaskRuntime, TaskRuntimeConfig, TaskSnapshot, TaskStatus,
};

use crate::adapter::{Adapter, FakeAdapter, HttpAdapter};
use crate::contract::{
    CameraCodec, CameraEncoder, CameraLifecycle, CameraState, CaptureDatasetRequest,
    CloseLiveViewRequest, CommandAcknowledgement, ConfigureWorldOutput, ConfigureWorldRequest,
    ControlGrantId, DurableOperation, ExecuteVehicleMissionPlanRequest, GrantVehicleControlRequest,
    MissionPlanId, OpenLiveViewRequest, PrepareVehicleMissionRequest, RenewLiveViewRequest,
    RevokeVehicleControlRequest, RunScenarioRequest, SessionId, SessionRequest, SimulationCommand,
    SimulationLifecycle, SimulationState, StepSimulationRequest, TakeoffRequest, TileLifecycle,
    TileState, VehicleControlPermission, VehicleId, VehicleRequest, VehicleState, Wgs84Position,
};
use crate::uris;

use super::auth::{InternalMcpAuthState, authenticate_internal_mcp};
use super::config::{AdapterKind, Args};
use super::control_authority::{ControlAuthorityError, VehicleControlAuthority};
use super::host::validate_host;
use super::live_view::{LiveViewConfig, LiveViewError, LiveViewService};
use super::live_view_audit::LiveViewAudit;
use super::ownership::{internal_caller, internal_identity, runtime_owner};
use super::prompts::UavSimPrompt;
use super::state::AppState;
use super::task_extension::UavSimTaskExtension;
use super::task_worker::{
    await_result, resume_queued_operation, start_operation, start_vehicle_mission_plan,
};

const SERVER_SLUG: &str = "uav-sim";
const LIST_PAGE_SIZE: usize = 100;
const LIVE_APP_TOOLS: &[&str] = &[
    "list_live_cameras",
    "open_live_view",
    "renew_live_view",
    "close_live_view",
];
const LIVE_APP_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNCIgaGVpZ2h0PSIyNCIgdmlld0JveD0iMCAwIDI0IDI0IiBmaWxsPSJub25lIiBzdHJva2U9IiM2NmU0ZmYiIHN0cm9rZS13aWR0aD0iMiI+PHJlY3QgeD0iMiIgeT0iNSIgd2lkdGg9IjIwIiBoZWlnaHQ9IjE0IiByeD0iMiIvPjxwYXRoIGQ9Im04IDlsNiAzLTYgM3oiLz48L3N2Zz4=";

fn live_app_resource(connect_origin: &str, agent_message_targets: &[String]) -> Resource {
    let resource = veoveo_mcp_apps_extension::app_resource_with_meta(
        uris::LIVE_APP_URI,
        "uav-sim-live-app",
        veoveo_mcp_apps_extension::ResourceUiMeta {
            csp: Some(veoveo_mcp_apps_extension::UiCsp {
                connect_domains: vec![connect_origin.to_owned()],
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .with_title("UAV live cameras")
    .with_description(
        "Authoritative simulator cameras tiled into one native NVIDIA NVENC product shared across viewers.",
    )
    .with_icons(vec![rmcp::model::Icon::new(LIVE_APP_ICON)]);
    if agent_message_targets.is_empty() {
        resource
    } else {
        veoveo_mcp_apps_extension::with_agent_message_targets(
            resource,
            agent_message_targets.iter().cloned(),
        )
        .expect("validated UAV App agent message targets")
    }
}

/// The crate documents embedded at build time and served under the well-known
/// surface: `uav-sim://docs`, `uav-sim://docs/{doc_id}`, `uav-sim://contract`,
/// and the administrative `admin/docs` routes (contract C18-C21).
pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!(SERVER_SLUG));
#[derive(Clone)]
struct UavSimMcp {
    state: Arc<AppState>,
    task_service: UavSimTaskExtension,
    #[allow(dead_code)]
    tool_router: ToolRouter<UavSimMcp>,
}

impl UavSimMcp {
    fn new(state: Arc<AppState>) -> Self {
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
        if identity_has_scope(identity, "uav-sim:read")
            || identity_has_scope(identity, "uav-sim:admin")
        {
            return Ok(state);
        }
        if !identity_has_scope(identity, "uav-sim:control") {
            return Err(McpError::invalid_request(
                "one of scopes uav-sim:read, uav-sim:control, uav-sim:admin is required",
                None,
            ));
        }
        let now = Utc::now();
        let visible_vehicle_ids = self
            .state
            .control_authority
            .visible_grants(identity, false)
            .await
            .map_err(authority_error)?
            .into_iter()
            .filter(|grant| {
                grant.session_id == state.session_id
                    && grant.revoked_at.is_none()
                    && grant.valid_from <= now
                    && grant.valid_until.is_none_or(|until| now < until)
                    && grant
                        .permissions
                        .contains(&VehicleControlPermission::Inspect)
            })
            .map(|grant| grant.vehicle_id)
            .collect::<BTreeSet<_>>();
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
        let identity = require_scope(context, "uav-sim:control")?;
        let state = self.state_for(session_id).await?;
        if !state
            .vehicles
            .iter()
            .any(|vehicle| &vehicle.vehicle_id == vehicle_id)
        {
            return Err(McpError::resource_not_found("vehicle not found", None));
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
        description = "Bind an unconfigured simulation session to one immutable Frames world revision and one static simulation frame.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ConfigureWorldOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn configure_world(
        &self,
        Parameters(request): Parameters<ConfigureWorldRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, "uav-sim:admin")?;
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
        description = "Read the current typed session, Google Photorealistic 3D Tiles, native sensor-stream health, recording, and vehicle state.",
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
            &["uav-sim:read", "uav-sim:control", "uav-sim:admin"],
        )?;
        let state = self.visible_state(&identity).await?;
        require_session(&state, request.session_id.as_str())?;
        structured_result("current UAV simulation state".to_owned(), &state)
    }

    #[tool(
        title = "List active vehicle control grants",
        description = "Read the active UAV-domain principal-to-vehicle grants visible to the authenticated caller in one session. Each grant supplies the exact vehicle id, permissions, and canonical Map mobility-profile URI that downstream Map route requests must use; caller input never establishes vehicle authority.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Vec<crate::contract::VehicleControlGrant>>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn list_active_vehicle_control_grants(
        &self,
        Parameters(request): Parameters<SessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_any_scope(&context, &["uav-sim:control", "uav-sim:admin"])?;
        self.state_for(&request.session_id).await?;
        let include_all = identity_has_scope(&identity, "uav-sim:admin");
        let grants = self
            .state
            .control_authority
            .active_visible_grants(&identity, &request.session_id, include_all)
            .await
            .map_err(authority_error)?;
        structured_result(
            format!("{} active vehicle control grant(s)", grants.len()),
            &grants,
        )
    }

    #[tool(
        title = "Grant vehicle control",
        description = "Bind one authenticated principal to one simulated vehicle with explicit UAV-domain permissions and one exact Map mobility profile.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::VehicleControlGrant>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn grant_vehicle_control(
        &self,
        Parameters(request): Parameters<GrantVehicleControlRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "uav-sim:admin")?;
        let state = self.state_for(&request.session_id).await?;
        if !state
            .vehicles
            .iter()
            .any(|vehicle| vehicle.vehicle_id == request.vehicle_id)
        {
            return Err(McpError::resource_not_found("vehicle not found", None));
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
        description = "Revoke one UAV-domain principal-to-vehicle grant using optimistic revision control.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::VehicleControlGrant>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn revoke_vehicle_control(
        &self,
        Parameters(request): Parameters<RevokeVehicleControlRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "uav-sim:admin")?;
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
        description = "Admit one Map-produced route handoff for the calling principal's granted vehicle and immutable Frames world revision.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::VehicleMissionPlan>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn prepare_vehicle_mission(
        &self,
        Parameters(request): Parameters<PrepareVehicleMissionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "uav-sim:control")?;
        let state = self.state_for(&request.session_id).await?;
        if !state
            .vehicles
            .iter()
            .any(|vehicle| vehicle.vehicle_id == request.vehicle_id)
        {
            return Err(McpError::resource_not_found("vehicle not found", None));
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
        description = "List the logical operator cameras rendered and encoded once inside the authoritative simulator for shared authorized viewing.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Vec<veoveo_mcp_contract::LiveCameraDescriptor>>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn list_live_cameras(
        &self,
        Parameters(request): Parameters<SessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, "uav-sim:stream")?;
        let state = self.state_for(&request.session_id).await?;
        structured_result(
            "authoritative UAV live cameras".to_owned(),
            &state.live_cameras,
        )
    }

    #[tool(
        title = "Open authoritative UAV live view",
        description = "Create one actor- and browser-instance-scoped authorization for an existing simulator-owned camera product. The authorization does not allocate another RTX render or NVENC encode.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_mcp_contract::LiveViewConnection>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true)
    )]
    async fn open_live_view(
        &self,
        Parameters(request): Parameters<OpenLiveViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "uav-sim:stream")?;
        let owner = LiveViewOwner::from_identity(&identity);
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
        description = "Rotate the stream token and extend the calling actor and browser instance's live-view authorization.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_mcp_contract::LiveViewConnection>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true)
    )]
    async fn renew_live_view(
        &self,
        Parameters(request): Parameters<RenewLiveViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "uav-sim:stream")?;
        let owner = LiveViewOwner::from_identity(&identity);
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
        description = "Revoke only the calling actor and browser instance's ephemeral live-view authorization. The shared camera product and other viewers remain active.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::CloseLiveViewResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn close_live_view(
        &self,
        Parameters(request): Parameters<CloseLiveViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "uav-sim:stream")?;
        let owner = LiveViewOwner::from_identity(&identity);
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
        require_scope(&context, "uav-sim:admin")?;
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
        require_scope(&context, "uav-sim:admin")?;
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
        require_scope(&context, "uav-sim:admin")?;
        self.apply_command(SimulationCommand::Reset(request)).await
    }

    #[tool(
        title = "Step UAV simulation",
        description = "Advance a paused session by a bounded number of physics steps.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommandAcknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn step_simulation(
        &self,
        Parameters(request): Parameters<StepSimulationRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, "uav-sim:admin")?;
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
        description = "Atomically arm one PX4-backed vehicle and start a bounded takeoff to a typed relative altitude.",
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
        description = "Run a bounded live scenario as a durable non-replayable task in the loaded Google Photorealistic 3D Tiles world.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::ScenarioResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn run_scenario(
        &self,
        Parameters(request): Parameters<RunScenarioRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, "uav-sim:admin")?;
        self.start_and_wait(&context, DurableOperation::RunScenario(request))
            .await
    }

    #[tool(
        title = "Execute vehicle mission plan",
        description = "Execute one admitted single-vehicle mission plan under the calling principal's exclusive UAV command lease.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::MissionResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn execute_vehicle_mission_plan(
        &self,
        Parameters(request): Parameters<ExecuteVehicleMissionPlanRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, "uav-sim:control")?;
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
        description = "Capture a bounded sensor interval as a durable non-replayable task and return governed recording identities.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::CaptureDatasetResult>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn capture_dataset(
        &self,
        Parameters(request): Parameters<CaptureDatasetRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_scope(&context, "uav-sim:admin")?;
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
        capabilities.extensions.get_or_insert_default().insert(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            rmcp::model::JsonObject::new(),
        );
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new(SERVER_SLUG, env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Govern UAV simulation sessions through typed resources and bounded controls. Map MCP owns operational geography and route handoffs; Frames owns immutable world revisions; this server owns principal-to-vehicle grants, mission admission, exclusive command leases, execution, telemetry, and the UAV App. Each logical operator camera renders and encodes continuously inside the authoritative simulator, while authenticated browser viewers share its H.264 product through ui://uav-sim/live.html. Use official Tasks for scenarios, admitted vehicle mission plans, and dataset captures; live operations are not replayed after an indeterminate interruption."
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
        let identity = internal_identity(&context)?;
        let state = self.visible_state(&identity).await?;
        let owner = runtime_owner(&identity);
        let tasks = self
            .state
            .tasks
            .list_for_owner(&owner)
            .await
            .map_err(internal)?;
        let mut resources = session_resources(&state);
        resources.extend(well_known_resources());
        if identity_has_scope(&identity, "uav-sim:control")
            || identity_has_scope(&identity, "uav-sim:admin")
        {
            let include_all = identity_has_scope(&identity, "uav-sim:admin");
            let grants = self
                .state
                .control_authority
                .visible_grants(&identity, include_all)
                .await
                .map_err(authority_error)?;
            let plans = self
                .state
                .control_authority
                .visible_plans(&identity, include_all)
                .await
                .map_err(authority_error)?;
            resources.push(descriptor(
                uris::CONTROL_GRANTS.to_owned(),
                "Vehicle control grants".to_owned(),
                "Caller-visible UAV principal-to-vehicle authority grants.",
            ));
            resources.push(descriptor(
                uris::MISSION_PLANS.to_owned(),
                "Vehicle mission plans".to_owned(),
                "Caller-visible UAV mission plans admitted from Map route handoffs.",
            ));
            resources.extend(grants.iter().map(|grant| {
                descriptor(
                    uris::control_grant(&grant.grant_id),
                    format!("Vehicle control grant {}", grant.grant_id),
                    "One UAV-owned principal-to-vehicle authority grant.",
                )
            }));
            resources.extend(plans.iter().map(|plan| {
                descriptor(
                    uris::mission_plan(&plan.plan_id),
                    format!("Vehicle mission plan {}", plan.plan_id),
                    "One UAV-owned mission plan admitted from a Map route handoff.",
                )
            }));
        }
        if identity_has_scope(&identity, "uav-sim:stream") {
            resources.push(live_app_resource(
                &self.state.live_view_connect_origin,
                &self.state.agent_message_targets,
            ));
            let live_session_id: LiveSessionId =
                state.session_id.as_str().parse().map_err(invalid)?;
            let owner = LiveViewOwner::from_identity(&identity);
            resources.extend(live_view_resources(
                &state,
                &self
                    .state
                    .live_views
                    .list(&owner, &identity.actor.id, &live_session_id)
                    .await,
            ));
        }
        resources.push(descriptor(
            uris::USAGE.to_owned(),
            "UAV simulation task usage".to_owned(),
            "Index of authorized task usage resources.",
        ));
        for task in &tasks {
            resources.push(descriptor(
                uris::usage_task(&task.task_id.to_string()),
                format!("Usage for task {}", task.task_id),
                "Usage report for one authorized UAV simulation task.",
            ));
            if let Some(mission_id) = mission_id(task) {
                resources.push(descriptor(
                    uris::mission(&mission_id),
                    format!("Mission {mission_id}"),
                    "Authorized durable mission task state.",
                ));
            }
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        resources.dedup_by(|left, right| left.uri == right.uri);
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
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        async {
            let uri = request.uri.as_str();
            if uri == uris::LIVE_APP_URI {
                require_scope(&context, "uav-sim:stream")?;
                return Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(uri, crate::live_app::html()),
                ]));
            }
            // Well-known surface (contract C18, C19): readable by any identity
            // that can list resources.
            if uri == uris::DOCS {
                internal_identity(&context)?;
                return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
            }
            if let Some(doc_id) = uris::parse_doc(uri) {
                internal_identity(&context)?;
                let doc = SERVER_DOCS.doc(doc_id).ok_or_else(|| {
                    McpError::resource_not_found("unknown UAV simulation document", None)
                })?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                ]));
            }
            if uri == uris::CONTRACT {
                internal_identity(&context)?;
                return json_resource(uri, SERVER_DOCS.contract_declaration());
            }
            if uri == uris::CONTROL_GRANTS || uris::parse_control_grant(uri).is_some() {
                let identity = require_any_scope(&context, &["uav-sim:control", "uav-sim:admin"])?;
                let grants = self
                    .state
                    .control_authority
                    .visible_grants(&identity, identity_has_scope(&identity, "uav-sim:admin"))
                    .await
                    .map_err(authority_error)?;
                if uri == uris::CONTROL_GRANTS {
                    return json_resource(uri, &grants);
                }
                let grant_id = ControlGrantId::new(
                    uris::parse_control_grant(uri).expect("checked control grant URI"),
                )
                .map_err(invalid)?;
                let grant = grants
                    .iter()
                    .find(|grant| grant.grant_id == grant_id)
                    .ok_or_else(|| McpError::resource_not_found("control grant not found", None))?;
                return json_resource(uri, grant);
            }
            if uri == uris::MISSION_PLANS || uris::parse_mission_plan(uri).is_some() {
                let identity = require_any_scope(&context, &["uav-sim:control", "uav-sim:admin"])?;
                let plans = self
                    .state
                    .control_authority
                    .visible_plans(&identity, identity_has_scope(&identity, "uav-sim:admin"))
                    .await
                    .map_err(authority_error)?;
                if uri == uris::MISSION_PLANS {
                    return json_resource(uri, &plans);
                }
                let plan_id = MissionPlanId::new(
                    uris::parse_mission_plan(uri).expect("checked mission plan URI"),
                )
                .map_err(invalid)?;
                let plan = plans
                    .iter()
                    .find(|plan| plan.plan_id == plan_id)
                    .ok_or_else(|| McpError::resource_not_found("mission plan not found", None))?;
                return json_resource(uri, plan);
            }
            let identity = require_any_scope(
                &context,
                &["uav-sim:read", "uav-sim:control", "uav-sim:admin"],
            )?;
            let state = self.visible_state(&identity).await?;
            if let Some(session_id) = uris::parse_live_cameras(uri) {
                require_scope(&context, "uav-sim:stream")?;
                require_session(&state, session_id.as_str())?;
                return json_resource(uri, &state.live_cameras);
            }
            if let Some((session_id, camera_id)) = uris::parse_live_camera(uri) {
                require_scope(&context, "uav-sim:stream")?;
                require_session(&state, session_id.as_str())?;
                let camera = state
                    .live_cameras
                    .iter()
                    .find(|camera| camera.camera_id == camera_id)
                    .ok_or_else(|| McpError::resource_not_found("live camera not found", None))?;
                return json_resource(uri, camera);
            }
            if let Some(session_id) = uris::parse_stream_products(uri) {
                require_scope(&context, "uav-sim:stream")?;
                require_session(&state, session_id.as_str())?;
                return json_resource(uri, &state.stream_products);
            }
            if let Some((session_id, product_id)) = uris::parse_stream_product(uri) {
                require_scope(&context, "uav-sim:stream")?;
                require_session(&state, session_id.as_str())?;
                let product = state
                    .stream_products
                    .iter()
                    .find(|product| product.stream_product_id == product_id)
                    .ok_or_else(|| {
                        McpError::resource_not_found("stream product not found", None)
                    })?;
                return json_resource(uri, product);
            }
            if let Some(session_id) = uris::parse_live_views(uri) {
                let identity = require_scope(&context, "uav-sim:stream")?;
                require_session(&state, session_id.as_str())?;
                let owner = LiveViewOwner::from_identity(&identity);
                let views = self
                    .state
                    .live_views
                    .list(&owner, &identity.actor.id, &session_id)
                    .await;
                return json_resource(uri, &views);
            }
            if let Some((session_id, live_view_id)) = uris::parse_live_view(uri) {
                let identity = require_scope(&context, "uav-sim:stream")?;
                require_session(&state, session_id.as_str())?;
                let owner = LiveViewOwner::from_identity(&identity);
                let view = self
                    .state
                    .live_views
                    .get(&owner, &identity.actor.id, &live_view_id)
                    .await
                    .map_err(live_view_error)?;
                return json_resource(uri, &view);
            }
            if uri == uris::SESSIONS {
                return json_resource(uri, &vec![session_summary(&state)]);
            }
            if let Some(session_id) = uris::parse_session(uri) {
                require_session(&state, session_id)?;
                return json_resource(uri, &state);
            }
            if let Some(session_id) = uris::parse_world(uri) {
                require_session(&state, session_id)?;
                return json_resource(uri, &world_view(&state));
            }
            if let Some(session_id) = uris::parse_tiles(uri) {
                require_session(&state, session_id)?;
                return json_resource(uri, &state.tiles);
            }
            if let Some(session_id) = uris::parse_vehicles(uri) {
                require_session(&state, session_id)?;
                return json_resource(uri, &state.vehicles);
            }
            if let Some((session_id, vehicle_id)) = uris::parse_vehicle(uri) {
                require_session(&state, session_id)?;
                let vehicle = state
                    .vehicles
                    .iter()
                    .find(|vehicle| vehicle.vehicle_id.as_str() == vehicle_id)
                    .ok_or_else(|| McpError::resource_not_found("vehicle not found", None))?;
                return json_resource(uri, vehicle);
            }
            if let Some(session_id) = uris::parse_recordings(uri) {
                require_session(&state, session_id)?;
                return json_resource(uri, &state.recordings);
            }
            let owner = runtime_owner(&internal_identity(&context)?);
            let tasks = self
                .state
                .tasks
                .list_for_owner(&owner)
                .await
                .map_err(internal)?;
            if uri == uris::USAGE {
                let values = tasks
                    .iter()
                    .map(|task| uris::usage_task(&task.task_id.to_string()))
                    .collect::<Vec<_>>();
                return json_resource(uri, &values);
            }
            if let Some(task_id) = uris::parse_usage_task(uri) {
                let task = require_task(&tasks, task_id)?;
                return json_resource(uri, &task_usage(task, uri));
            }
            if let Some(value) = uris::parse_mission(uri) {
                let requested_mission_id =
                    crate::contract::MissionId::new(value).map_err(invalid)?;
                let task = tasks
                    .iter()
                    .find(|task| mission_id(task).as_ref() == Some(&requested_mission_id))
                    .ok_or_else(|| McpError::resource_not_found("mission not found", None))?;
                return json_resource(uri, task);
            }
            Err(McpError::resource_not_found(
                format!("unknown UAV simulation resource `{uri}`"),
                None,
            ))
        }
        .await
        .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
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
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let identity = internal_identity(&context)?;
        let state = self.visible_state(&identity).await?;
        let owner = runtime_owner(&identity);
        let tasks = self
            .state
            .tasks
            .list_for_owner(&owner)
            .await
            .map_err(internal)?;
        let values = match (reference.uri.as_str(), request.argument.name.as_str()) {
            (uris::SESSION_TEMPLATE, "session_id")
            | (uris::WORLD_TEMPLATE, "session_id")
            | (uris::TILES_TEMPLATE, "session_id")
            | (uris::VEHICLES_TEMPLATE, "session_id")
            | (uris::RECORDINGS_TEMPLATE, "session_id")
            | (uris::LIVE_CAMERAS_TEMPLATE, "session_id")
            | (uris::LIVE_CAMERA_TEMPLATE, "session_id")
            | (uris::STREAM_PRODUCTS_TEMPLATE, "session_id")
            | (uris::STREAM_PRODUCT_TEMPLATE, "session_id")
            | (uris::LIVE_VIEWS_TEMPLATE, "session_id")
            | (uris::LIVE_VIEW_TEMPLATE, "session_id")
            | (uris::VEHICLE_TEMPLATE, "session_id") => vec![state.session_id.to_string()],
            (uris::VEHICLE_TEMPLATE, "vehicle_id") => state
                .vehicles
                .iter()
                .map(|vehicle| vehicle.vehicle_id.to_string())
                .collect(),
            (uris::LIVE_CAMERA_TEMPLATE, "camera_id") => state
                .live_cameras
                .iter()
                .map(|camera| camera.camera_id.to_string())
                .collect(),
            (uris::STREAM_PRODUCT_TEMPLATE, "product_id") => state
                .stream_products
                .iter()
                .map(|product| product.stream_product_id.to_string())
                .collect(),
            (uris::MISSION_TEMPLATE, "mission_id") => tasks
                .iter()
                .filter_map(mission_id)
                .map(|id| id.to_string())
                .collect(),
            (uris::CONTROL_GRANT_TEMPLATE, "grant_id") => {
                require_any_identity_scope(&identity, &["uav-sim:control", "uav-sim:admin"])?;
                self.state
                    .control_authority
                    .visible_grants(&identity, identity_has_scope(&identity, "uav-sim:admin"))
                    .await
                    .map_err(authority_error)?
                    .into_iter()
                    .map(|grant| grant.grant_id.to_string())
                    .collect()
            }
            (uris::MISSION_PLAN_TEMPLATE, "plan_id") => {
                require_any_identity_scope(&identity, &["uav-sim:control", "uav-sim:admin"])?;
                self.state
                    .control_authority
                    .visible_plans(&identity, identity_has_scope(&identity, "uav-sim:admin"))
                    .await
                    .map_err(authority_error)?
                    .into_iter()
                    .map(|plan| plan.plan_id.to_string())
                    .collect()
            }
            (uris::USAGE_TASK_TEMPLATE, "task_id") => {
                tasks.iter().map(|task| task.task_id.to_string()).collect()
            }
            (uris::DOC_TEMPLATE, "doc_id") => {
                SERVER_DOCS.iter().map(|doc| doc.id.to_owned()).collect()
            }
            _ => return Ok(CompleteResult::default()),
        };
        complete_values(values, &request.argument.value)
    }
}

impl UavSimMcp {
    async fn require_subscribable(
        &self,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = internal_identity(context)?;
        let state = self.visible_state(&identity).await?;
        if uri == uris::CONTROL_GRANTS || uris::parse_control_grant(uri).is_some() {
            let identity = require_any_scope(context, &["uav-sim:control", "uav-sim:admin"])?;
            if uri == uris::CONTROL_GRANTS {
                return Ok(());
            }
            let requested = uris::parse_control_grant(uri).expect("checked control grant URI");
            if self
                .state
                .control_authority
                .visible_grants(&identity, identity_has_scope(&identity, "uav-sim:admin"))
                .await
                .map_err(authority_error)?
                .iter()
                .any(|grant| grant.grant_id.as_str() == requested)
            {
                return Ok(());
            }
            return Err(McpError::resource_not_found(
                "control grant not found",
                None,
            ));
        }
        if uri == uris::MISSION_PLANS || uris::parse_mission_plan(uri).is_some() {
            let identity = require_any_scope(context, &["uav-sim:control", "uav-sim:admin"])?;
            if uri == uris::MISSION_PLANS {
                return Ok(());
            }
            let requested = uris::parse_mission_plan(uri).expect("checked mission plan URI");
            if self
                .state
                .control_authority
                .visible_plans(&identity, identity_has_scope(&identity, "uav-sim:admin"))
                .await
                .map_err(authority_error)?
                .iter()
                .any(|plan| plan.plan_id.as_str() == requested)
            {
                return Ok(());
            }
            return Err(McpError::resource_not_found("mission plan not found", None));
        }
        if let Some(session_id) = live_session_from_subscribable(uri) {
            require_scope(context, "uav-sim:stream")?;
            require_session(&state, session_id.as_str())?;
            if let Some((_, camera_id)) = uris::parse_live_camera(uri)
                && !state
                    .live_cameras
                    .iter()
                    .any(|camera| camera.camera_id == camera_id)
            {
                return Err(McpError::resource_not_found("live camera not found", None));
            }
            if let Some((_, product_id)) = uris::parse_stream_product(uri)
                && !state
                    .stream_products
                    .iter()
                    .any(|product| product.stream_product_id == product_id)
            {
                return Err(McpError::resource_not_found(
                    "stream product not found",
                    None,
                ));
            }
            if let Some((_, live_view_id)) = uris::parse_live_view(uri) {
                let identity = internal_identity(context)?;
                let owner = LiveViewOwner::from_identity(&identity);
                self.state
                    .live_views
                    .get(&owner, &identity.actor.id, &live_view_id)
                    .await
                    .map_err(live_view_error)?;
            }
            return Ok(());
        }
        if let Some(session_id) = session_from_subscribable(uri) {
            require_session(&state, session_id)?;
            if let Some((_, vehicle_id)) = uris::parse_vehicle(uri)
                && !state
                    .vehicles
                    .iter()
                    .any(|vehicle| vehicle.vehicle_id.as_str() == vehicle_id)
            {
                return Err(McpError::resource_not_found("vehicle not found", None));
            }
            return Ok(());
        }
        if let Some(mission) = uris::parse_mission(uri) {
            let owner = runtime_owner(&internal_identity(context)?);
            let tasks = self
                .state
                .tasks
                .list_for_owner(&owner)
                .await
                .map_err(internal)?;
            if tasks
                .iter()
                .filter_map(mission_id)
                .any(|id| id.as_str() == mission)
            {
                return Ok(());
            }
        }
        Err(McpError::resource_not_found(
            "resource is not subscribable",
            None,
        ))
    }
}

pub(super) async fn serve() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-uav-sim-mcp", "info,veoveo_uav_sim_mcp=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            args.surreal_endpoint.clone(),
            args.surreal_namespace.clone(),
            args.surreal_database.clone(),
            args.surreal_auth_level,
            args.surreal_username.clone(),
            args.surreal_password.clone(),
        ),
        SERVER_SLUG,
        format!("{SERVER_SLUG}-{}", uuid::Uuid::now_v7()),
    )
    .await?;
    let recovery = tasks.recover().await?;
    let control_authority = VehicleControlAuthority::new(tasks.platform_store().clone());
    let adapter = match args.adapter {
        AdapterKind::Http => Adapter::Http(Box::new(HttpAdapter::new(
            args.adapter_url()?,
            args.adapter_timeout()?,
            args.adapter_operation_timeout()?,
            args.adapter_bearer_token.clone(),
            tasks.platform_store().clone(),
            &args.recording_tenant_key,
        )?)),
        AdapterKind::Fake => Adapter::Fake(Arc::new(Mutex::new(FakeAdapter::new(fake_state()?)))),
    };
    let adapter = Arc::new(adapter);
    if let Some(path) = args.world_bootstrap_file.as_deref() {
        super::world_bootstrap::apply(path, &adapter).await?;
    }
    let runtime_session_id = LiveSessionId::new(adapter.state().await?.session_id.to_string())?;
    let live_view_audit = LiveViewAudit::new(tasks.platform_store().clone());
    let live_views = LiveViewService::new(
        adapter.clone(),
        live_view_audit.clone(),
        LiveViewConfig {
            session_duration: args.live_view_session_duration()?,
            public_stream_url: args.public_stream_url.clone(),
            maximum_frame_age_ms: args.live_view_maximum_frame_age_ms,
        },
    )?;
    let stream_url = url::Url::parse(&args.public_stream_url)?;
    let live_view_connect_origin = stream_url.origin().ascii_serialization();
    anyhow::ensure!(
        live_view_connect_origin != "null",
        "public live-stream URL must have an HTTP(S) origin"
    );
    let subscribers = Arc::new(SubscriptionHub::new());
    let agent_message_targets = if args.agent_message_targets.is_empty() {
        Vec::new()
    } else {
        let resource = veoveo_mcp_apps_extension::with_agent_message_targets(
            veoveo_mcp_apps_extension::app_resource(uris::LIVE_APP_URI, "uav-sim-live-app"),
            args.agent_message_targets.iter().cloned(),
        )
        .ok_or_else(|| anyhow::anyhow!("UAV_SIM_AGENT_MESSAGE_TARGETS is invalid"))?;
        veoveo_mcp_apps_extension::resource_agent_message_targets(&resource)
    };
    let runtime_event_listener = (args.adapter == AdapterKind::Http).then(|| {
        super::runtime_events::RuntimeEventListener::new(
            runtime_session_id,
            args.world_bootstrap_file.clone(),
            adapter.clone(),
        )
    });
    let state = Arc::new(AppState {
        adapter,
        tasks,
        control_authority,
        subscribers: subscribers.clone(),
        live_views: live_views.clone(),
        live_view_audit,
        live_view_connect_origin,
        agent_message_targets,
    });
    for snapshot in recovery.resumable {
        resume_queued_operation(state.clone(), snapshot)
            .await
            .map_err(anyhow::Error::msg)?;
    }

    let shutdown = CancellationToken::new();
    let runtime_event_task = runtime_event_listener
        .map(|listener| tokio::spawn(listener.run(subscribers.clone(), shutdown.child_token())));
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts);
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(UavSimMcp::new(state.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(shutdown.child_token()),
    );
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn_with_state(
            InternalMcpAuthState {
                verifier: verifier.clone(),
            },
            authenticate_internal_mcp,
        ));
    // Read-only well-known projection (contract C20) behind the same gateway
    // authentication as the MCP surface.
    let admin_router = super::admin::router().layer(middleware::from_fn_with_state(
        InternalMcpAuthState { verifier },
        authenticate_internal_mcp,
    ));
    anyhow::ensure!(
        args.live_stream_gate_port != args.port,
        "live-stream gate port must differ from the MCP port"
    );
    let live_stream_gate = super::live_stream::LiveStreamGate::new(
        live_views,
        subscribers,
        &args.public_stream_url,
        &args.runtime_stream_url,
        args.adapter_bearer_token.clone(),
    )?;
    let router = Router::new()
        .nest(
            public_endpoint.mount_path(),
            Router::new()
                .route("/healthz", get(|| async { "ok" }))
                .route("/readyz", get(ready))
                .nest("/admin", admin_router)
                .nest("/mcp", mcp_router),
        )
        .with_state(state)
        .layer(middleware::from_fn_with_state(allowed_hosts, validate_host))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    let live_stream_address = SocketAddr::from(([0, 0, 0, 0], args.live_stream_gate_port));
    tracing::info!(%address, public_url = public_endpoint.public_url(), "UAV simulation MCP listening");
    let listener = tokio::net::TcpListener::bind(address).await?;
    let server = std::future::IntoFuture::into_future(
        axum::serve(listener, router).with_graceful_shutdown({
            let shutdown = shutdown.clone();
            async move {
                let _ = tokio::signal::ctrl_c().await;
                shutdown.cancel();
            }
        }),
    );
    let mut live_stream_task =
        tokio::spawn(live_stream_gate.run(live_stream_address, shutdown.child_token()));
    tokio::pin!(server);
    let result = tokio::select! {
        result = &mut server => result.map_err(Into::into),
        result = &mut live_stream_task => match result {
            Ok(result) => result,
            Err(error) => Err(error.into()),
        },
    };
    shutdown.cancel();
    if !live_stream_task.is_finished() {
        live_stream_task.await??;
    }
    if let Some(task) = runtime_event_task {
        task.await?;
    }
    result
}

async fn ready(State(state): State<Arc<AppState>>) -> StatusCode {
    match state.adapter.state().await {
        Ok(simulation) if simulation.lifecycle != SimulationLifecycle::Failed => StatusCode::OK,
        Ok(_) => StatusCode::SERVICE_UNAVAILABLE,
        Err(error) => {
            tracing::warn!(%error, "UAV simulation MCP readiness failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

pub(crate) fn fake_state() -> anyhow::Result<SimulationState> {
    let revision_uri = veoveo_mcp_contract::FrameWorldRevisionUri::new(
        &veoveo_mcp_contract::FrameWorldId::new("test-world")?,
        &veoveo_mcp_contract::FrameWorldRevisionId::new("revision-1")?,
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
            simulation_frame_uri: veoveo_mcp_contract::WorldFrameUri::new(
                &revision_uri,
                &veoveo_mcp_contract::FrameId::new("isaac-world")?,
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
        live_cameras: vec![veoveo_mcp_contract::LiveCameraDescriptor {
            camera_id: veoveo_mcp_contract::LiveCameraId::new("follow")?,
            session_id: LiveSessionId::new("session-alpha")?,
            revision: 1,
            rig: veoveo_mcp_contract::LiveCameraRig::FollowEntity {
                target_entity_id: veoveo_mcp_contract::LiveEntityId::new("uav-1")?,
                eye_offset_flu_m: veoveo_mcp_contract::LiveVector3 {
                    x: -8.0,
                    y: 0.0,
                    z: 3.0,
                },
                target_offset_flu_m: veoveo_mcp_contract::LiveVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.2,
                },
                smoothing: veoveo_mcp_contract::LiveCameraSmoothing {
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
            stream_policy: veoveo_mcp_contract::LiveCameraStreamPolicy::Continuous,
            health: veoveo_mcp_contract::LiveCameraHealth::Healthy,
            last_frame_at: Some(Utc::now()),
        }],
        stream_products: vec![veoveo_mcp_contract::LiveStreamProductState {
            stream_product_id: veoveo_mcp_contract::LiveStreamProductId::new("camera-atlas")?,
            camera_regions: vec![veoveo_mcp_contract::LiveCameraRegion {
                camera_id: veoveo_mcp_contract::LiveCameraId::new("follow")?,
                x_px: 0,
                y_px: 0,
                width_px: 1_280,
                height_px: 720,
            }],
            coded_width_px: 1_280,
            coded_height_px: 720,
            lifecycle: veoveo_mcp_contract::LiveStreamProductLifecycle::Ready,
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

fn session_resources(state: &SimulationState) -> Vec<Resource> {
    let session_id = &state.session_id;
    let mut resources = vec![
        descriptor(
            uris::SESSIONS.to_owned(),
            "UAV simulation sessions".to_owned(),
            "Authorized simulation session index.",
        ),
        descriptor(
            uris::session(session_id),
            format!("Session {session_id}"),
            "Typed simulation session state.",
        ),
        descriptor(
            uris::world(session_id),
            format!("World {session_id}"),
            "Frame, georeference, and simulation clock.",
        ),
        descriptor(
            uris::tiles(session_id),
            format!("Tiles {session_id}"),
            "Google Photorealistic 3D Tiles load state inside the simulator.",
        ),
        descriptor(
            uris::vehicles(session_id),
            format!("Vehicles {session_id}"),
            "Vehicle inventory for one simulation session.",
        ),
        descriptor(
            uris::recordings(session_id),
            format!("Recordings {session_id}"),
            "Governed recording identities emitted by the session.",
        ),
        descriptor(
            uris::live_cameras(
                &session_id
                    .as_str()
                    .parse()
                    .expect("session IDs share the live-view identifier profile"),
            ),
            format!("Live cameras {session_id}"),
            "Authoritative operator-camera inventory.",
        ),
        descriptor(
            uris::stream_products(
                &session_id
                    .as_str()
                    .parse()
                    .expect("session IDs share the live-view identifier profile"),
            ),
            format!("Stream products {session_id}"),
            "Stable one-per-camera render and NVIDIA NVENC product inventory.",
        ),
    ];
    resources.extend(state.vehicles.iter().map(|vehicle| {
        descriptor(
            uris::vehicle(session_id, &vehicle.vehicle_id),
            format!("Vehicle {}", vehicle.vehicle_id),
            "Typed simulated vehicle state.",
        )
    }));
    resources
}

/// Well-known surface resources (contract C18, C19). `list_resources` serves
/// these for every authorized identity; `capability_inventory` declares the
/// same URIs at `uav-sim://contract`.
fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![descriptor(
        uris::DOCS.to_owned(),
        "Server documents".to_owned(),
        "Index of the crate documents embedded at build time.",
    )];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(uris::doc(doc.id), doc.title)
                .with_title(doc.title)
                .with_description("Crate document embedded at build time.")
                .with_mime_type("text/markdown"),
        );
    }
    resources.push(descriptor(
        uris::CONTRACT.to_owned(),
        "Contract declaration".to_owned(),
        "Machine-readable contract revision, compliance, and capability inventory.",
    ));
    resources
}

/// Every advertised resource template. `list_resource_templates` serves this
/// list and the `uav-sim://contract` capability inventory declares it, so the
/// two cannot diverge.
fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(uris::DOC_TEMPLATE, "Server document")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        template(
            uris::SESSION_TEMPLATE,
            "Simulation session",
            "Typed session state.",
        ),
        template(
            uris::WORLD_TEMPLATE,
            "Simulation world",
            "Frame, georeference, and world clock state.",
        ),
        template(
            uris::TILES_TEMPLATE,
            "Simulation tiles",
            "Google Photorealistic 3D Tiles load state inside the simulator.",
        ),
        template(
            uris::VEHICLES_TEMPLATE,
            "Simulation vehicles",
            "Vehicle inventory for one session.",
        ),
        template(
            uris::VEHICLE_TEMPLATE,
            "Simulation vehicle",
            "Typed state for one simulated vehicle.",
        ),
        template(
            uris::RECORDINGS_TEMPLATE,
            "Simulation recordings",
            "Governed recording identities emitted by one session.",
        ),
        template(
            uris::LIVE_CAMERAS_TEMPLATE,
            "Live cameras",
            "Authoritative operator-camera inventory.",
        ),
        template(
            uris::LIVE_CAMERA_TEMPLATE,
            "Live camera",
            "One authoritative operator camera.",
        ),
        template(
            uris::STREAM_PRODUCTS_TEMPLATE,
            "Stream products",
            "Stable camera-owned rendered and encoded products.",
        ),
        template(
            uris::STREAM_PRODUCT_TEMPLATE,
            "Stream product",
            "One camera-owned RTX render and NVIDIA NVENC product shared across viewers.",
        ),
        template(
            uris::LIVE_VIEWS_TEMPLATE,
            "Live views",
            "Caller-visible live-view authorizations without secret tokens.",
        ),
        template(
            uris::LIVE_VIEW_TEMPLATE,
            "Live view",
            "One caller-visible live-view authorization without its token.",
        ),
        template(
            uris::MISSION_TEMPLATE,
            "Simulation mission",
            "Authorized durable mission task state.",
        ),
        template(
            uris::CONTROL_GRANT_TEMPLATE,
            "Vehicle control grant",
            "One UAV-owned principal-to-vehicle authority grant.",
        ),
        template(
            uris::MISSION_PLAN_TEMPLATE,
            "Vehicle mission plan",
            "One UAV-owned plan admitted from a Map route handoff.",
        ),
        template(
            uris::USAGE_TASK_TEMPLATE,
            "Simulation task usage",
            "Usage report for one authorized task.",
        ),
    ]
}

fn descriptor(uri: String, title: String, description: &str) -> Resource {
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

fn session_summary(state: &SimulationState) -> serde_json::Value {
    json!({
        "session_id": state.session_id,
        "lifecycle": state.lifecycle,
        "world": state.world,
        "tile_lifecycle": state.tiles.lifecycle,
        "vehicle_count": state.vehicles.len(),
        "recording_count": state.recordings.len(),
        "timing": state.timing,
        "updated_at": state.updated_at,
    })
}

fn world_view(state: &SimulationState) -> serde_json::Value {
    json!({
        "session_id": state.session_id,
        "simulation_time_s": state.simulation_time_s,
        "physics_step": state.physics_step,
        "timing": state.timing,
        "world": state.world,
        "updated_at": state.updated_at,
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

fn mission_id(task: &TaskSnapshot) -> Option<crate::contract::MissionId> {
    match serde_json::from_value::<DurableOperation>(task.request.clone()).ok()? {
        DurableOperation::ExecuteMission(request) => Some(request.mission_id),
        _ => None,
    }
}

fn task_usage(task: &TaskSnapshot, uri: &str) -> UsageReport {
    let operation = serde_json::from_value::<DurableOperation>(task.request.clone()).ok();
    let declared_duration = match operation.as_ref() {
        Some(DurableOperation::RunScenario(request)) => Some(request.duration_seconds),
        Some(DurableOperation::CaptureDataset(request)) => Some(request.duration_seconds),
        Some(DurableOperation::ExecuteMission(_)) | None => None,
    };
    let completed_duration = task
        .started_at
        .zip(task.completed_at)
        .map(|(started, completed)| (completed - started).num_milliseconds() as f64 / 1_000.0);
    let (kind, quantity) = if task.status == TaskStatus::Succeeded {
        (UsageKind::Actual, completed_duration.or(declared_duration))
    } else {
        (UsageKind::Estimate, declared_duration)
    };
    UsageReport::new(task.task_id.to_string(), uri).with_records(vec![UsageRecord {
        task_id: task.task_id.to_string(),
        source_id: None,
        provider_job_id: None,
        model_id: "isaac-sim-6.0.1".to_owned(),
        kind,
        quantity,
        unit: Some("gpu_second".to_owned()),
        amount: None,
        currency: None,
        recorded_at: task.completed_at.unwrap_or(task.updated_at),
        metadata: json!({"gpu_count": 1, "task_type": task.task_type}),
    }])
}

fn require_task<'a>(
    tasks: &'a [TaskSnapshot],
    task_id: &str,
) -> Result<&'a TaskSnapshot, McpError> {
    tasks
        .iter()
        .find(|task| task.task_id.to_string() == task_id)
        .ok_or_else(|| McpError::resource_not_found("task not found", None))
}

fn require_session(state: &SimulationState, session_id: &str) -> Result<(), McpError> {
    if state.session_id.as_str() == session_id {
        Ok(())
    } else {
        Err(McpError::resource_not_found(
            "simulation session not found",
            None,
        ))
    }
}

fn session_from_subscribable(uri: &str) -> Option<&str> {
    uris::parse_session(uri)
        .or_else(|| uris::parse_world(uri))
        .or_else(|| uris::parse_tiles(uri))
        .or_else(|| uris::parse_vehicles(uri))
        .or_else(|| uris::parse_recordings(uri))
        .or_else(|| uris::parse_vehicle(uri).map(|(session_id, _)| session_id))
}

fn live_session_from_subscribable(uri: &str) -> Option<LiveSessionId> {
    uris::parse_live_cameras(uri)
        .or_else(|| uris::parse_live_camera(uri).map(|(session, _)| session))
        .or_else(|| uris::parse_stream_products(uri))
        .or_else(|| uris::parse_stream_product(uri).map(|(session, _)| session))
        .or_else(|| uris::parse_live_views(uri))
        .or_else(|| uris::parse_live_view(uri).map(|(session, _)| session))
}

fn live_view_resources(
    state: &SimulationState,
    views: &[veoveo_mcp_contract::LiveViewState],
) -> Vec<Resource> {
    let session_id: LiveSessionId = state
        .session_id
        .as_str()
        .parse()
        .expect("session IDs share the live-view identifier profile");
    let mut resources = vec![
        descriptor(
            uris::live_cameras(&session_id),
            format!("Live cameras {session_id}"),
            "Authoritative operator-camera inventory.",
        ),
        descriptor(
            uris::stream_products(&session_id),
            format!("Stream products {session_id}"),
            "Stable one-per-camera rendered and encoded product inventory.",
        ),
        descriptor(
            uris::live_views(&session_id),
            format!("Live views {session_id}"),
            "Caller-visible live-view authorizations without secret tokens.",
        ),
    ];
    resources.extend(state.live_cameras.iter().map(|camera| {
        descriptor(
            uris::live_camera(&session_id, &camera.camera_id),
            format!("Live camera {}", camera.camera_id),
            "One authoritative operator camera.",
        )
    }));
    resources.extend(state.stream_products.iter().map(|product| {
        descriptor(
            uris::stream_product(&session_id, &product.stream_product_id),
            format!("Stream product {}", product.stream_product_id),
            "One stable camera render and NVIDIA NVENC product.",
        )
    }));
    resources.extend(views.iter().map(|view| {
        descriptor(
            uris::live_view(&session_id, &view.live_view_id),
            format!("Live view {}", view.live_view_id),
            "One caller-visible live-view authorization without its token.",
        )
    }));
    resources
}

fn require_scope(
    context: &RequestContext<RoleServer>,
    required: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    identity_has_scope(&identity, required)
        .then_some(identity)
        .ok_or_else(|| McpError::invalid_request(format!("scope `{required}` is required"), None))
}

fn require_any_scope(
    context: &RequestContext<RoleServer>,
    required: &[&str],
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    require_any_identity_scope(&identity, required)?;
    Ok(identity)
}

fn require_any_identity_scope(
    identity: &GatewayInternalIdentity,
    required: &[&str],
) -> Result<(), McpError> {
    required
        .iter()
        .any(|required| identity_has_scope(identity, required))
        .then_some(())
        .ok_or_else(|| {
            McpError::invalid_request(
                format!("one of scopes {} is required", required.join(", ")),
                None,
            )
        })
}

fn authority_error(error: ControlAuthorityError) -> McpError {
    match error {
        ControlAuthorityError::Invalid(message) => McpError::invalid_params(message, None),
        ControlAuthorityError::Forbidden => {
            McpError::invalid_request("vehicle control is not authorized", None)
        }
        ControlAuthorityError::NotFound => {
            McpError::resource_not_found("vehicle control record not found", None)
        }
        ControlAuthorityError::Conflict | ControlAuthorityError::VehicleBusy(_) => {
            McpError::invalid_request(error.to_string(), None)
        }
        ControlAuthorityError::Store(_)
        | ControlAuthorityError::Database(_)
        | ControlAuthorityError::Json(_) => McpError::internal_error(error.to_string(), None),
    }
}

fn identity_has_scope(identity: &GatewayInternalIdentity, required: &str) -> bool {
    identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
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
    live_view_id: Option<&veoveo_mcp_contract::LiveViewId>,
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
            McpError::invalid_request("live-view access is not authorized", None)
        }
        LiveViewError::CameraUnavailable | LiveViewError::ViewUnavailable => {
            McpError::invalid_request(error.to_string(), None)
        }
        _ => McpError::internal_error(error.to_string(), None),
    }
}

fn complete_values(values: Vec<String>, needle: &str) -> Result<CompleteResult, McpError> {
    let needle = needle.to_lowercase();
    let mut matches = values
        .into_iter()
        .filter(|value| value.to_lowercase().contains(&needle))
        .collect::<Vec<_>>();
    matches.sort();
    matches.dedup();
    let total = matches.len();
    matches.truncate(CompletionInfo::MAX_VALUES);
    let completion = CompletionInfo::with_pagination(
        matches,
        Some(total as u32),
        total > CompletionInfo::MAX_VALUES,
    )
    .map_err(internal)?;
    Ok(CompleteResult::new(completion))
}

fn structured_result<T: Serialize>(message: String, value: &T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(message)]);
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
        let text = serde_json::to_string(&world_view(&fake_state().unwrap())).unwrap();
        assert!(!text.contains("token"));
        assert!(!text.contains("CESIUM_ION_ACCESS_TOKEN"));
    }

    #[test]
    fn live_app_uses_the_default_complete_console_content_workspace() {
        let resource = live_app_resource("wss://stream.example.com", &["uav-1-pilot".to_owned()]);
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
        assert!(crate::live_app::html().contains("hardwareAcceleration=\"prefer-hardware\""));
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
