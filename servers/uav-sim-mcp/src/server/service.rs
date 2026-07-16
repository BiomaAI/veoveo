use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, extract::State, http::StatusCode, middleware, routing::get};
use chrono::Utc;
use clap::Parser;
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
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::Serialize;
use serde_json::json;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle, Page,
    ServerSlug, SubscriptionHub, TelemetryGuard, TokenIssuer, UsageKind, UsageRecord, UsageReport,
    init_server_telemetry, paginate, public_allowed_hosts,
};
use veoveo_mcp_task_extension::{
    Implementation as TaskExtensionImplementation, ServerDiscovery, TaskExtensionAdapter,
    task_extension_middleware,
};
use veoveo_task_runtime::{
    TaskRetentionPin, TaskRuntime, TaskRuntimeConfig, TaskSnapshot, TaskStatus,
};

use crate::adapter::{Adapter, FakeAdapter, HttpAdapter};
use crate::contract::{
    CaptureDatasetRequest, CommandAcknowledgement, DurableOperation, ExecuteMissionRequest,
    RunScenarioRequest, SessionId, SessionRequest, SimulationCommand, SimulationLifecycle,
    SimulationState, StepSimulationRequest, TakeoffRequest, TileLifecycle, TileState, VehicleId,
    VehicleRequest, VehicleState, Wgs84Position,
};
use crate::uris;

use super::auth::{InternalMcpAuthState, authenticate_internal_mcp};
use super::config::{AdapterKind, Args};
use super::host::validate_host;
use super::ownership::{internal_caller, internal_identity, runtime_owner};
use super::prompts::UavSimPrompt;
use super::state::AppState;
use super::task_extension::UavSimTaskExtension;
use super::task_worker::{await_result, resume_queued_operation, start_operation};

const SERVER_SLUG: &str = "uav-sim";
const LIST_PAGE_SIZE: usize = 100;

#[derive(Clone)]
struct UavSimMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<UavSimMcp>,
}

impl UavSimMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    async fn current_state(&self) -> Result<SimulationState, McpError> {
        self.state
            .adapter
            .lock()
            .await
            .state()
            .await
            .map_err(internal)
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

    async fn apply_command(&self, command: SimulationCommand) -> Result<CallToolResult, McpError> {
        let result = self
            .state
            .adapter
            .lock()
            .await
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
        title = "Get UAV simulation state",
        description = "Read the current typed session, Google Photorealistic 3D Tiles, recording, and vehicle state.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SimulationState>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn get_simulation_state(
        &self,
        Parameters(request): Parameters<SessionRequest>,
    ) -> Result<CallToolResult, McpError> {
        let state = self.state_for(&request.session_id).await?;
        structured_result("current UAV simulation state".to_owned(), &state)
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
    ) -> Result<CallToolResult, McpError> {
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
    ) -> Result<CallToolResult, McpError> {
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
    ) -> Result<CallToolResult, McpError> {
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
    ) -> Result<CallToolResult, McpError> {
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
    ) -> Result<CallToolResult, McpError> {
        self.apply_command(SimulationCommand::Arm(request)).await
    }

    #[tool(
        title = "Take off simulated UAV",
        description = "Start a bounded takeoff to a typed relative altitude.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CommandAcknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn takeoff_vehicle(
        &self,
        Parameters(request): Parameters<TakeoffRequest>,
    ) -> Result<CallToolResult, McpError> {
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
    ) -> Result<CallToolResult, McpError> {
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
        self.start_and_wait(&context, DurableOperation::RunScenario(request))
            .await
    }

    #[tool(
        title = "Execute UAV mission",
        description = "Execute typed multi-vehicle waypoints as a durable non-replayable task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::MissionResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn execute_mission(
        &self,
        Parameters(request): Parameters<ExecuteMissionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let state = self.state_for(&request.session_id).await?;
        if request.frame_uri != state.frame_uri {
            return Err(McpError::invalid_params(
                "mission frame_uri does not match the session frame",
                None,
            ));
        }
        self.start_and_wait(&context, DurableOperation::ExecuteMission(request))
            .await
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
        self.start_and_wait(&context, DurableOperation::CaptureDataset(request))
            .await
    }
}

#[tool_handler]
impl ServerHandler for UavSimMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_completions()
            .build();
        info.server_info = rmcp::model::Implementation::new(SERVER_SLUG, env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Govern UAV simulation sessions through typed resources and bounded controls. Google Photorealistic 3D Tiles readiness inside the simulation is part of session state. Use the final task extension for scenarios, missions, and dataset captures; live operations are not replayed after an indeterminate interruption."
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
        let state = self.current_state().await?;
        let owner = runtime_owner(&internal_identity(&context)?);
        let tasks = self
            .state
            .tasks
            .list_for_owner(&owner)
            .await
            .map_err(internal)?;
        let mut resources = session_resources(&state);
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
                uris::MISSION_TEMPLATE,
                "Simulation mission",
                "Authorized durable mission task state.",
            ),
            template(
                uris::USAGE_TASK_TEMPLATE,
                "Simulation task usage",
                "Usage report for one authorized task.",
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
        let uri = request.uri.as_str();
        let state = self.current_state().await?;
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
            let requested_mission_id = crate::contract::MissionId::new(value).map_err(invalid)?;
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
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        UavSimPrompt::by_name(&request.name)
            .ok_or_else(|| McpError::invalid_params("unknown UAV simulation prompt", None))?
            .render(request.arguments)
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.require_subscribable(&request.uri, &context).await?;
        let identity = internal_identity(&context)?;
        self.state
            .subscribers
            .subscribe(request.uri, identity.principal.id, context.peer.clone())
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.require_subscribable(&request.uri, &context).await?;
        let identity = internal_identity(&context)?;
        self.state
            .subscribers
            .unsubscribe(&request.uri, &identity.principal.id)
            .await;
        Ok(())
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let state = self.current_state().await?;
        let owner = runtime_owner(&internal_identity(&context)?);
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
            | (uris::VEHICLE_TEMPLATE, "session_id") => vec![state.session_id.to_string()],
            (uris::VEHICLE_TEMPLATE, "vehicle_id") => state
                .vehicles
                .iter()
                .map(|vehicle| vehicle.vehicle_id.to_string())
                .collect(),
            (uris::MISSION_TEMPLATE, "mission_id") => tasks
                .iter()
                .filter_map(mission_id)
                .map(|id| id.to_string())
                .collect(),
            (uris::USAGE_TASK_TEMPLATE, "task_id") => {
                tasks.iter().map(|task| task.task_id.to_string()).collect()
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
        let state = self.current_state().await?;
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
    let adapter = match args.adapter {
        AdapterKind::Http => Adapter::Http(HttpAdapter::new(
            args.adapter_url()?,
            args.adapter_timeout()?,
            args.adapter_operation_timeout()?,
            tasks.platform_store().clone(),
            &args.recording_tenant_key,
        )?),
        AdapterKind::Fake => Adapter::Fake(FakeAdapter::new(fake_state()?)),
    };
    let state = Arc::new(AppState {
        adapter: Arc::new(Mutex::new(adapter)),
        tasks,
        subscribers: SubscriptionHub::new(),
    });
    for snapshot in recovery.resumable {
        resume_queued_operation(state.clone(), snapshot)
            .await
            .map_err(anyhow::Error::msg)?;
    }

    let shutdown = CancellationToken::new();
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
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_stateful_mode(false)
            .with_json_response(true)
            .with_cancellation_token(shutdown.child_token()),
    );
    let extension = Arc::new(TaskExtensionAdapter::new(
        Arc::new(UavSimTaskExtension::new(state.clone())),
        ServerDiscovery::new(
            BTreeMap::from([
                ("tools".to_owned(), json!({})),
                ("resources".to_owned(), json!({"subscribe": true})),
                ("prompts".to_owned(), json!({})),
                ("completions".to_owned(), json!({})),
            ]),
            TaskExtensionImplementation {
                name: SERVER_SLUG.to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some("Durable UAV simulation tasks and typed resource subscriptions.".to_owned()),
        ),
    ));
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            extension,
            task_extension_middleware::<UavSimTaskExtension>,
        ))
        .layer(middleware::from_fn_with_state(
            InternalMcpAuthState { verifier },
            authenticate_internal_mcp,
        ));
    let router = Router::new()
        .nest(
            public_endpoint.mount_path(),
            Router::new()
                .route("/healthz", get(|| async { "ok" }))
                .route("/readyz", get(ready))
                .nest("/mcp", mcp_router),
        )
        .with_state(state)
        .layer(middleware::from_fn_with_state(allowed_hosts, validate_host))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(%address, public_url = public_endpoint.public_url(), "UAV simulation MCP listening");
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown({
            let shutdown = shutdown.clone();
            async move {
                let _ = tokio::signal::ctrl_c().await;
                shutdown.cancel();
            }
        })
        .await?;
    Ok(())
}

async fn ready(State(state): State<Arc<AppState>>) -> StatusCode {
    match state.adapter.lock().await.state().await {
        Ok(simulation)
            if simulation.lifecycle != SimulationLifecycle::Failed
                && simulation.tiles.lifecycle == TileLifecycle::Ready =>
        {
            StatusCode::OK
        }
        Ok(_) => StatusCode::SERVICE_UNAVAILABLE,
        Err(error) => {
            tracing::warn!(%error, "UAV simulation MCP readiness failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

fn fake_state() -> anyhow::Result<SimulationState> {
    Ok(SimulationState {
        session_id: SessionId::new("session-alpha")?,
        lifecycle: SimulationLifecycle::Ready,
        simulation_time_s: 0.0,
        physics_step: 0,
        frame_uri: "frames://frame/enu-alpha".to_owned(),
        georeference_origin: Wgs84Position {
            latitude_degrees: 13.6929,
            longitude_degrees: -89.2182,
            ellipsoid_height_m: 700.0,
        },
        tiles: TileState {
            lifecycle: TileLifecycle::Ready,
            source: "google_photorealistic_3d_tiles".to_owned(),
            ion_asset_id: 1,
            resident_tiles: 20,
            loading_tiles: 0,
            failed_tiles: 0,
            diagnostic: None,
        },
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
        "frame_uri": state.frame_uri,
        "tile_lifecycle": state.tiles.lifecycle,
        "vehicle_count": state.vehicles.len(),
        "recording_count": state.recordings.len(),
        "updated_at": state.updated_at,
    })
}

fn world_view(state: &SimulationState) -> serde_json::Value {
    json!({
        "session_id": state.session_id,
        "simulation_time_s": state.simulation_time_s,
        "physics_step": state.physics_step,
        "frame_uri": state.frame_uri,
        "georeference_origin": state.georeference_origin,
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
    fn fake_delivery_has_core_google_tiles_and_px4() {
        let state = fake_state().unwrap();
        assert_eq!(state.tiles.lifecycle, TileLifecycle::Ready);
        assert_eq!(state.tiles.source, "google_photorealistic_3d_tiles");
        assert!(state.vehicles.iter().all(|vehicle| vehicle.px4_connected));
    }

    #[test]
    fn world_view_never_contains_a_credential() {
        let text = serde_json::to_string(&world_view(&fake_state().unwrap())).unwrap();
        assert!(!text.contains("token"));
        assert!(!text.contains("CESIUM_ION_ACCESS_TOKEN"));
    }
}
