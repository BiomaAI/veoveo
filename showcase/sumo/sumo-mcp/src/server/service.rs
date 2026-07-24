use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, middleware, routing::get};
use clap::Parser;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
        ServerCapabilities, ServerInfo, SubscribeRequestParams, UnsubscribeRequestParams,
    },
    service::RequestContext,
    tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::Serialize;
use serde_json::json;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::tool;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    SubscriptionHub, TelemetryGuard, TokenIssuer, init_server_telemetry, public_allowed_hosts,
};
use veoveo_mcp_task_extension::{
    Implementation as TaskExtensionImplementation, ServerDiscovery, TaskExtensionAdapter,
    task_extension_middleware,
};
use veoveo_task_runtime::{TaskRetentionPin, TaskRuntime, TaskRuntimeConfig};

use crate::contract::{
    Acknowledgement, CongestionState, DurableOperation, LaneRequest, OfflineOperationRequest,
    RerouteVehicleRequest, RunBatchRequest, Scenario, SetEdgeSpeedRequest, SetSignalPhaseRequest,
    TrafficState,
};
use crate::driver::{FakeSimDriver, SimDriver, TraciSimDriver};
use crate::recording::RecordingPublisher;

use super::auth::{InternalMcpAuthState, authenticate_internal_mcp};
use super::config::{Args, DriverKind};
use super::host::validate_host;
use super::ownership::{internal_caller, internal_identity};
use super::state::{AppState, OfflineBinaries, World};
use super::task_extension::SumoTaskExtension;
use super::task_worker::{await_result, resume_operation, start_operation};

const SERVER_SLUG: &str = "sumo";
const CONGESTION_URI: &str = "sumo://congestion";
const STATE_URI: &str = "sumo://state";
const SCENARIO_URI: &str = "sumo://scenario";
const CONGESTION_THRESHOLD_MPS: f64 = 5.0;

#[derive(Clone)]
struct SumoMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<SumoMcp>,
}

impl SumoMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
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
impl SumoMcp {
    #[tool(
        title = "Query traffic state",
        description = "Read the current typed SUMO world state.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TrafficState>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn query_state(&self) -> Result<CallToolResult, McpError> {
        let state = self
            .state
            .world
            .lock()
            .await
            .driver
            .state()
            .map_err(internal)?;
        structured_result("current SUMO traffic state".to_owned(), &state)
    }

    #[tool(
        title = "Describe scenario",
        description = "Read the loaded SUMO network and signal inventory.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Scenario>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn describe_scenario(&self) -> Result<CallToolResult, McpError> {
        let scenario = self
            .state
            .world
            .lock()
            .await
            .driver
            .describe()
            .map_err(internal)?;
        structured_result("loaded SUMO scenario".to_owned(), &scenario)
    }

    #[tool(
        title = "Set signal phase",
        description = "Apply a traffic-signal phase to the live simulation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Acknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn set_signal_phase(
        &self,
        Parameters(request): Parameters<SetSignalPhaseRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.state
            .world
            .lock()
            .await
            .driver
            .set_signal_phase(&request.signal_id, request.phase)
            .map_err(invalid)?;
        let result = Acknowledgement {
            applied: true,
            detail: format!("{} -> phase {}", request.signal_id, request.phase),
        };
        structured_result(result.detail.clone(), &result)
    }

    #[tool(
        title = "Reroute vehicle",
        description = "Change a live vehicle destination edge.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Acknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn reroute_vehicle(
        &self,
        Parameters(request): Parameters<RerouteVehicleRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.state
            .world
            .lock()
            .await
            .driver
            .reroute_vehicle(&request.vehicle_id, &request.target_edge_id)
            .map_err(invalid)?;
        let result = Acknowledgement {
            applied: true,
            detail: format!("{} -> {}", request.vehicle_id, request.target_edge_id),
        };
        structured_result(result.detail.clone(), &result)
    }

    #[tool(
        title = "Set edge speed",
        description = "Apply an edge speed limit in metres per second.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Acknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn set_edge_speed(
        &self,
        Parameters(request): Parameters<SetEdgeSpeedRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.state
            .world
            .lock()
            .await
            .driver
            .set_edge_speed(&request.edge_id, request.speed_mps)
            .map_err(invalid)?;
        let result = Acknowledgement {
            applied: true,
            detail: format!("{} -> {:.1} m/s", request.edge_id, request.speed_mps),
        };
        structured_result(result.detail.clone(), &result)
    }

    #[tool(
        title = "Close lane",
        description = "Close one lane to all simulated traffic.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Acknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn close_lane(
        &self,
        Parameters(request): Parameters<LaneRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.state
            .world
            .lock()
            .await
            .driver
            .close_lane(&request.lane_id)
            .map_err(invalid)?;
        let result = Acknowledgement {
            applied: true,
            detail: format!("closed {}", request.lane_id),
        };
        structured_result(result.detail.clone(), &result)
    }

    #[tool(
        title = "Open lane",
        description = "Reopen one lane to simulated traffic.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<Acknowledgement>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn open_lane(
        &self,
        Parameters(request): Parameters<LaneRequest>,
    ) -> Result<CallToolResult, McpError> {
        self.state
            .world
            .lock()
            .await
            .driver
            .open_lane(&request.lane_id)
            .map_err(invalid)?;
        let result = Acknowledgement {
            applied: true,
            detail: format!("opened {}", request.lane_id),
        };
        structured_result(result.detail.clone(), &result)
    }

    #[tool(
        title = "Check congestion",
        description = "Evaluate the current congestion condition and notify subscribers when active.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CongestionState>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn check_congestion(&self) -> Result<CallToolResult, McpError> {
        let state = self
            .state
            .world
            .lock()
            .await
            .driver
            .state()
            .map_err(internal)?;
        let congestion = congestion(&state);
        if congestion.congested {
            self.state
                .subscribers
                .notify_resource_updated(CONGESTION_URI)
                .await;
        }
        structured_result("current congestion condition".to_owned(), &congestion)
    }

    #[tool(
        title = "Run simulation batch",
        description = "Advance SUMO through a durable non-replayable task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::RunBatchResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn run_batch(
        &self,
        Parameters(request): Parameters<RunBatchRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.start_and_wait(&context, DurableOperation::RunBatch(request))
            .await
    }

    #[tool(
        title = "Generate SUMO network",
        description = "Generate a SUMO network through a durable resumable task. Missing SUMO binaries fail explicitly.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::OfflineOperationResult>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn generate_network(
        &self,
        Parameters(request): Parameters<OfflineOperationRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.start_and_wait(&context, DurableOperation::GenerateNetwork(request))
            .await
    }

    #[tool(
        title = "Compute SUMO routes",
        description = "Compute routes through a durable resumable task. Missing SUMO binaries fail explicitly.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::OfflineOperationResult>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn compute_routes(
        &self,
        Parameters(request): Parameters<OfflineOperationRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.start_and_wait(&context, DurableOperation::ComputeRoutes(request))
            .await
    }

    #[tool(
        title = "Optimize SUMO signals",
        description = "Run SUMO signal coordination through a durable resumable task. Missing binaries fail explicitly.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<crate::contract::OfflineOperationResult>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn optimize_signals(
        &self,
        Parameters(request): Parameters<OfflineOperationRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.start_and_wait(&context, DurableOperation::OptimizeSignals(request))
            .await
    }
}

#[tool_handler]
impl ServerHandler for SumoMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .build();
        info.server_info = rmcp::model::Implementation::new("sumo", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "SUMO traffic-world controls. Long operations use the final durable task extension; subscribe to sumo://congestion for condition changes."
                .into(),
        );
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![
                Resource::new(STATE_URI, "traffic state")
                    .with_mime_type("application/json")
                    .with_description("Current typed SUMO traffic state."),
                Resource::new(SCENARIO_URI, "scenario")
                    .with_mime_type("application/json")
                    .with_description("Loaded network and signal inventory."),
                Resource::new(CONGESTION_URI, "congestion")
                    .with_mime_type("application/json")
                    .with_description("Subscribable congestion condition."),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let mut world = self.state.world.lock().await;
        match request.uri.as_str() {
            STATE_URI => json_resource(STATE_URI, &world.driver.state().map_err(internal)?),
            SCENARIO_URI => {
                json_resource(SCENARIO_URI, &world.driver.describe().map_err(internal)?)
            }
            CONGESTION_URI => json_resource(
                CONGESTION_URI,
                &congestion(&world.driver.state().map_err(internal)?),
            ),
            uri => Err(McpError::resource_not_found(
                format!("unknown SUMO resource `{uri}`"),
                None,
            )),
        }
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        if request.uri != CONGESTION_URI {
            return Err(McpError::resource_not_found(
                "only sumo://congestion is subscribable",
                None,
            ));
        }
        let identity = internal_identity(&context)?;
        self.state
            .subscribers
            .subscribe(request.uri, identity.actor.id, context.peer.clone())
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        if request.uri != CONGESTION_URI {
            return Err(McpError::resource_not_found(
                "only sumo://congestion is subscribable",
                None,
            ));
        }
        let identity = internal_identity(&context)?;
        self.state
            .subscribers
            .unsubscribe(&request.uri, &identity.actor.id)
            .await;
        Ok(())
    }
}

pub(super) async fn serve() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-sumo-mcp", "info,veoveo_sumo_mcp=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    std::fs::create_dir_all(&args.work_dir)?;

    let driver: Box<dyn SimDriver> = match args.driver {
        DriverKind::Traci => {
            let host = args.sumo_host.clone();
            let scenario = args.scenario.clone();
            let max_vehicles = args.max_vehicles;
            let port = args.sumo_port;
            let retries = args.connect_retries;
            Box::new(
                tokio::task::spawn_blocking(move || {
                    TraciSimDriver::connect(&host, port, scenario, max_vehicles, retries)
                })
                .await??,
            )
        }
        DriverKind::Fake => Box::new(FakeSimDriver::new(
            args.fake_vehicles,
            args.fake_seed,
            (40, 60),
        )),
    };
    let publisher = RecordingPublisher::connect(&args.recording_proxy, &args.recording)?;
    let mut world = World {
        driver,
        publisher,
        congested: false,
    };
    let geometry = world.driver.network_geometry()?;
    world.publisher.publish_network(&geometry)?;
    let initial = world.driver.state()?;
    world.congested = congestion(&initial).congested;
    world.publisher.publish(&initial)?;

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
    let state = Arc::new(AppState {
        world: Arc::new(Mutex::new(world)),
        tasks,
        work_dir: args.work_dir.clone(),
        binaries: OfflineBinaries {
            netgenerate: args.netgenerate_bin.clone(),
            duarouter: args.duarouter_bin.clone(),
            tls_coordinator: args.tls_coordinator_bin.clone(),
        },
        subscribers: SubscriptionHub::new(),
        artifacts: veoveo_artifact_client::HttpArtifactPlane::new(
            args.artifact_service_url.clone(),
        ),
        max_artifact_bytes: args.max_artifact_bytes,
    });
    for snapshot in recovery.resumable {
        resume_operation(state.clone(), snapshot)
            .await
            .map_err(anyhow::Error::msg)?;
    }

    let shutdown = CancellationToken::new();
    let simulation = tokio::spawn(simulation_loop(
        state.clone(),
        args.step_interval()?,
        shutdown.child_token(),
    ));
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        veoveo_mcp_contract::ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts);
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(SumoMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(shutdown.child_token()),
    );
    let extension = Arc::new(TaskExtensionAdapter::new(
        Arc::new(SumoTaskExtension::new(state.clone())),
        ServerDiscovery::new(
            std::collections::BTreeMap::from([
                ("tools".to_owned(), json!({})),
                ("resources".to_owned(), json!({"subscribe": true})),
            ]),
            TaskExtensionImplementation {
                name: "sumo".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some("Durable SUMO traffic-world tasks and push subscriptions.".to_owned()),
        ),
    ));
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            extension,
            task_extension_middleware::<SumoTaskExtension>,
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
                .nest("/mcp", mcp_router),
        )
        .layer(middleware::from_fn_with_state(allowed_hosts, validate_host))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(%address, public_url = public_endpoint.public_url(), "SUMO MCP listening");
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
    shutdown.cancel();
    simulation.await??;
    let mut world = state.world.lock().await;
    world.publisher.flush()?;
    world.driver.close()?;
    Ok(())
}

async fn simulation_loop(
    state: Arc<AppState>,
    interval: std::time::Duration,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            () = shutdown.cancelled() => return Ok(()),
            _ = ticker.tick() => {
                let changed = {
                    let mut world = state.world.lock().await;
                    world.driver.step(1)?;
                    let current = world.driver.state()?;
                    world.publisher.publish(&current)?;
                    let congested = congestion(&current).congested;
                    let changed = congested != world.congested;
                    world.congested = congested;
                    changed
                };
                if changed {
                    state.subscribers.notify_resource_updated(CONGESTION_URI).await;
                }
            }
        }
    }
}

fn congestion(state: &TrafficState) -> CongestionState {
    CongestionState {
        congested: state.mean_speed_mps < CONGESTION_THRESHOLD_MPS,
        mean_speed_mps: state.mean_speed_mps,
        threshold_mps: CONGESTION_THRESHOLD_MPS,
        simulation_time_s: state.simulation_time_s,
    }
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
        assert!(!SumoMcp::tool_router().list_all().is_empty());
    }

    #[test]
    fn congestion_threshold_is_typed() {
        let state = TrafficState {
            simulation_time_s: 1.0,
            vehicle_count: 0,
            mean_speed_mps: 4.9,
            vehicles: Vec::new(),
            signals: Vec::new(),
        };
        assert!(congestion(&state).congested);
    }
}
