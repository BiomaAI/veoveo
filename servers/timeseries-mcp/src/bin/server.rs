//! Timeseries MCP server.
//!
//! MCP surface:
//!   tool `forecast(source, mapping, horizon)` — final task-extension execution
//!   template `timeseries://artifact/{artifact_id}` — Rerun RRD artifact bytes
//!   template `timeseries://usage/task/{task_id}` — task usage rows

use std::{
    collections::BTreeSet,
    net::SocketAddr,
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
    time::Duration,
};

use axum::{Router, middleware, routing::get};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{TimeDelta, Utc};
use clap::Parser;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ResourceTemplate, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_duckdb_runtime::HttpsSourcePolicy;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability, Page, ServerSlug,
    TelemetryGuard, TokenIssuer, UsageReport, init_server_telemetry, paginate,
    public_allowed_hosts,
};
use veoveo_mcp_task_extension::{
    Implementation as TaskExtensionImplementation, ServerDiscovery, TaskExtensionAdapter,
    task_extension_middleware,
};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, RecoveryClass, TaskError, TaskFailure, TaskId,
    TaskPayloadState, TaskRuntime, TaskRuntimeConfig, TaskSnapshot, TaskTransition,
};
use veoveo_timeseries_mcp::{
    artifacts::ArtifactRepository,
    contract::{TimeseriesForecastOutput, TimeseriesForecastRequest},
    forecast::{RRD_MIME_TYPE, run_forecast},
    uris,
};

#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/config.rs"]
mod config;
#[path = "server/host.rs"]
mod host;
#[path = "server/internal_auth.rs"]
mod internal_auth;
#[path = "server/outputs.rs"]
mod outputs;
#[path = "server/ownership.rs"]
mod ownership;
#[path = "server/task_extension.rs"]
mod task_extension;

use app_state::{AppState, update_task};
use config::Args;
use host::validate_host;
use internal_auth::{InternalMcpAuthState, authenticate_internal_mcp};
use outputs::{forecast_result, usage_record};
use ownership::{
    internal_caller, internal_identity, optional_task_owner, require_task_owner, runtime_owner,
    task_owner_allows, task_owner_from_identity, task_owner_from_runtime,
};
use task_extension::TimeseriesTaskExtension;

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3000;
const MCP_TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);
const SERVER_SLUG: &str = "timeseries";
const LIST_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ForecastTaskRequest {
    input: TimeseriesForecastRequest,
    artifact_write_capability: IssuedArtifactWriteCapability,
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Clone)]
struct TimeseriesMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<TimeseriesMcp>,
}

#[tool_router]
impl TimeseriesMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Forecast timeseries",
        description = "Ingest a DuckDB-readable time-series source, compute a forecast, and return one timeseries://artifact/{artifact_id} Rerun RRD artifact.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TimeseriesForecastOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn forecast(
        &self,
        Parameters(args): Parameters<TimeseriesForecastRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let caller = internal_caller(&context)?;
        let identity = internal_identity(&context)?;
        let snapshot = start_forecast_task(
            self.state.clone(),
            identity,
            caller,
            args,
            Some(TaskProgress {
                peer: context.peer.clone(),
                token: context.meta.get_progress_token(),
            }),
            BTreeSet::new(),
        )
        .await
        .map_err(|err| McpError::internal_error(err, None))?;
        let task_id = snapshot.task_id.to_string();

        match self
            .state
            .tasks
            .await_payload_state(&task_id)
            .await
            .map_err(|err| McpError::internal_error(err.to_string(), None))?
        {
            TaskPayloadState::Completed(payload) => {
                serde_json::from_value(payload).map_err(|err| {
                    McpError::internal_error(
                        format!("invalid persisted forecast task result: {err}"),
                        None,
                    )
                })
            }
            TaskPayloadState::Failed(error) => Err(McpError::internal_error(
                error.message,
                Some(json!({"code": error.code, "details": error.details})),
            )),
            TaskPayloadState::Cancelled => {
                Err(McpError::invalid_request("forecast was cancelled", None))
            }
            TaskPayloadState::Running => Err(McpError::internal_error(
                "forecast task wait ended while still running",
                None,
            )),
            TaskPayloadState::Unknown => Err(McpError::internal_error(
                "forecast task disappeared before completion",
                None,
            )),
        }
    }
}

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE)
        .map_err(|err| McpError::invalid_params(err.to_string(), None))
}

#[tool_handler]
impl ServerHandler for TimeseriesMcp {
    fn get_info(&self) -> ServerInfo {
        let mut caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        veoveo_mcp_apps_extension::extend_capabilities(&mut caps);
        let mut info = ServerInfo::default();
        info.capabilities = caps;
        info.server_info =
            rmcp::model::Implementation::new("timeseries", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Timeseries forecasting server. Call `forecast` with a typed DuckDB source and table \
             mapping; the result contains a timeseries://artifact/{artifact_id} Rerun RRD output."
                .into(),
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
        // The #[tool] macro has no meta attribute; the app link is attached
        // to the listed tool here.
        tools = tools
            .into_iter()
            .map(|tool| {
                if tool.name == "forecast" {
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        tool,
                        uris::FORECAST_APP_URI,
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
        let mut resources = vec![
            veoveo_mcp_apps_extension::app_resource(uris::FORECAST_APP_URI, "forecast-app")
                .with_title("Timeseries forecast view")
                .with_description(
                    "Interactive MCP App rendering forecast previews and re-running the \
                     forecast tool.",
                ),
            Resource::new(uris::USAGE_ROOT_URI, "usage")
                .with_title("Timeseries usage ledger")
                .with_description("Index of task usage resources.")
                .with_mime_type("application/json"),
        ];
        // Artifacts live on the shared plane now; this server keeps no local
        // artifact index to enumerate. They remain readable by their
        // artifact URI through resources/read, which resolves against the plane.
        for task_id in self
            .state
            .tasks
            .platform_store()
            .domain_usage_task_ids(SERVER_SLUG)
            .await
            .map_err(|err| McpError::internal_error(err.to_string(), None))?
        {
            let task_id = task_id.to_string();
            let Some(owner) = optional_task_owner(&self.state, &task_id).await? else {
                continue;
            };
            if !task_owner_allows(&owner, &identity) {
                continue;
            }
            resources.push(
                Resource::new(
                    uris::usage_task_uri(&task_id),
                    format!("usage for task {task_id}"),
                )
                .with_description("Usage rows for one timeseries task.")
                .with_mime_type("application/json"),
            );
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
            ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
                .with_title("Timeseries artifact")
                .with_description(
                    "Server-owned immutable Rerun RRD artifact, addressed by occurrence id.",
                )
                .with_mime_type(RRD_MIME_TYPE),
            ResourceTemplate::new(uris::USAGE_TASK_TEMPLATE, "usage")
                .with_title("Timeseries task usage")
                .with_description("Usage rows for one task, addressed by task id.")
                .with_mime_type("application/json"),
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
        let identity = internal_identity(&context)?;
        let uri = request.uri.as_str();
        if uri == uris::FORECAST_APP_URI {
            return Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(
                    uri,
                    include_str!("../../assets/forecast-app.html"),
                ),
            ]));
        }
        if uri == uris::USAGE_ROOT_URI {
            let mut entries = Vec::new();
            for task_id in self
                .state
                .tasks
                .platform_store()
                .domain_usage_task_ids(SERVER_SLUG)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?
            {
                let task_id = task_id.to_string();
                let Some(owner) = optional_task_owner(&self.state, &task_id).await? else {
                    continue;
                };
                if task_owner_allows(&owner, &identity) {
                    entries.push(json!({
                        "task_id": task_id,
                        "usage_uri": uris::usage_task_uri(&task_id),
                    }));
                }
            }
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(serde_json::to_string(&entries).unwrap_or_default(), uri)
                    .with_mime_type("application/json"),
            ]));
        }
        if let Some(task_id) = uris::parse_usage_task_uri(uri) {
            require_task_owner(&self.state, &context, task_id).await?;
            let durable_task_id = task_id
                .parse::<TaskId>()
                .map_err(|err| McpError::invalid_params(format!("invalid task id: {err}"), None))?;
            let records = self
                .state
                .tasks
                .platform_store()
                .domain_usage_for_task(SERVER_SLUG, durable_task_id)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?
                .into_iter()
                .map(|record| usage_record(task_id, record))
                .collect::<Vec<_>>();
            if records.is_empty() {
                return Err(McpError::resource_not_found(
                    format!("unknown usage task '{task_id}'"),
                    None,
                ));
            }
            let report = UsageReport::new(task_id, uri).with_records(records);
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(serde_json::to_string(&report).unwrap_or_default(), uri)
                    .with_mime_type("application/json"),
            ]));
        }
        if let Some(artifact_id) = uris::parse_artifact_uri(uri) {
            // The plane enforces access with the caller's identity.
            let caller = internal_caller(&context)?;
            let artifact = self
                .state
                .artifacts
                .get(&caller, &artifact_id)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown artifact '{artifact_id}'"), None)
                })?;
            let blob = BASE64_STANDARD.encode(&artifact.bytes);
            let mut content = ResourceContents::blob(blob, uri);
            content = content.with_mime_type(
                artifact
                    .metadata
                    .mime_type
                    .unwrap_or_else(|| RRD_MIME_TYPE.to_string()),
            );
            return Ok(ReadResourceResult::new(vec![content]));
        }
        Err(McpError::resource_not_found(
            format!("unknown resource uri: {uri}"),
            None,
        ))
    }
}

struct TaskProgress {
    peer: rmcp::service::Peer<RoleServer>,
    token: Option<rmcp::model::ProgressToken>,
}

async fn start_forecast_task(
    state: Arc<AppState>,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: veoveo_mcp_contract::PlaneCaller,
    args: TimeseriesForecastRequest,
    progress: Option<TaskProgress>,
    retention_pins: BTreeSet<veoveo_task_runtime::TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let task_id = TaskId::new();
    let artifact_write_capability = state
        .artifacts
        .issue_write_capability(
            &caller,
            &IssueArtifactWriteCapabilityRequest {
                task_id: task_id.to_string(),
                expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                max_artifact_count: NonZeroU32::new(1).expect("one artifact is non-zero"),
                max_total_bytes: NonZeroU64::new(state.max_artifact_bytes)
                    .ok_or_else(|| "max artifact bytes must be non-zero".to_owned())?,
            },
        )
        .await
        .map_err(|err| err.to_string())?;
    let request = ForecastTaskRequest {
        input: args,
        artifact_write_capability,
    };
    let created = state
        .tasks
        .create(DurableCreateTask {
            task_id,
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type: "forecast".to_owned(),
            request: serde_json::to_value(&request).map_err(|err| err.to_string())?,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(MCP_TASK_TTL_MS),
            poll_interval_ms: Some(MCP_TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|err| err.to_string())?;
    schedule_forecast_task(
        state,
        created.snapshot,
        request,
        task_owner_from_identity(&task_id.to_string(), &identity),
        progress,
    )
    .await
    .map_err(|err| err.to_string())
}

async fn schedule_forecast_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: ForecastTaskRequest,
    owner: veoveo_timeseries_mcp::state::TaskOwner,
    progress: Option<TaskProgress>,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state.tasks.claim(&task_id, TASK_LEASE_DURATION).await?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id.clone(),
        request,
        owner,
        progress,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn resume_forecast_task(state: Arc<AppState>, snapshot: TaskSnapshot) -> anyhow::Result<()> {
    let request: ForecastTaskRequest = serde_json::from_value(snapshot.request.clone())?;
    let task_id = snapshot.task_id.to_string();
    let owner = task_owner_from_runtime(&task_id, &snapshot.owner).map_err(anyhow::Error::msg)?;
    schedule_forecast_task(state, snapshot, request, owner, None)
        .await
        .map(|_| ())
}

async fn notify_task_progress(progress: &Option<TaskProgress>, value: f64, message: &str) {
    if let Some(progress) = progress {
        veoveo_mcp_contract::notify_progress(&progress.peer, &progress.token, value, message).await;
    }
}

async fn complete_tool_error(state: &AppState, task_id: &str, message: String) {
    let result = CallToolResult::error(vec![ContentBlock::text(message.clone())]);
    let transition = match serde_json::to_value(result) {
        Ok(result) => TaskTransition::Succeeded { message, result },
        Err(error) => TaskTransition::Failed(TaskFailure::new(
            "result_serialization_failed",
            error.to_string(),
        )),
    };
    update_task(state, task_id, transition).await;
}

async fn run_task(
    state: Arc<AppState>,
    task_id: String,
    request: ForecastTaskRequest,
    owner: veoveo_timeseries_mcp::state::TaskOwner,
    progress: Option<TaskProgress>,
    cancellation: CancellationToken,
) {
    let work = run_task_inner(
        state.clone(),
        task_id.clone(),
        request,
        owner,
        progress,
        cancellation.clone(),
    );
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(TASK_LEASE_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            () = &mut work => break,
            _ = heartbeat.tick() => {
                if let Err(error) = state.tasks.renew_lease(&task_id, TASK_LEASE_DURATION).await {
                    tracing::warn!(task_id, "task lease heartbeat failed: {error}");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
}

async fn run_task_inner(
    state: Arc<AppState>,
    task_id: String,
    request: ForecastTaskRequest,
    owner: veoveo_timeseries_mcp::state::TaskOwner,
    progress: Option<TaskProgress>,
    cancellation: CancellationToken,
) {
    macro_rules! fail {
        ($msg:expr) => {{
            let msg: String = $msg;
            tracing::warn!(task_id, "timeseries task failed: {msg}");
            complete_tool_error(&state, &task_id, msg).await;
            return;
        }};
    }
    notify_task_progress(&progress, 0.1, "materializing source").await;
    let artifact = match tokio::task::spawn_blocking({
        let task_id = task_id.clone();
        let input = request.input.clone();
        let source_policy = state.source_policy.clone();
        move || run_forecast(&task_id, &input, &source_policy)
    })
    .await
    {
        Ok(Ok(artifact)) => artifact,
        Ok(Err(err)) => fail!(format!("forecast failed: {err}")),
        Err(err) => fail!(format!("forecast worker failed: {err}")),
    };
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    notify_task_progress(&progress, 0.8, "writing artifact").await;
    let result = match forecast_result(
        &state,
        &request.artifact_write_capability,
        &task_id,
        &owner,
        artifact,
    )
    .await
    {
        Ok(result) => result,
        Err(err) => fail!(format!("artifact write failed: {err}")),
    };
    notify_task_progress(&progress, 1.0, "completed").await;
    let payload = match serde_json::to_value(&result) {
        Ok(payload) => payload,
        Err(err) => fail!(format!("serializing forecast result failed: {err}")),
    };
    update_task(
        &state,
        &task_id,
        TaskTransition::Succeeded {
            message: "completed; RRD artifact available".to_owned(),
            result: payload,
        },
    )
    .await;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-timeseries-mcp", "info,veoveo_timeseries_mcp=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let internal_token_verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let artifacts = ArtifactRepository::new(args.artifact_service_url.clone());
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
    let mut source_policy = HttpsSourcePolicy::new(args.allow_source_hosts.clone());
    source_policy.max_bytes = args.max_source_bytes;
    let state = Arc::new(AppState {
        tasks,
        artifacts,
        source_policy,
        max_artifact_bytes: args.max_artifact_bytes,
    });
    for snapshot in recovery.resumable {
        if let Err(error) = resume_forecast_task(state.clone(), snapshot).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(task_id, "another replica claimed recovered forecast task");
                }
                _ => return Err(error),
            }
        }
    }

    let ct = tokio_util::sync::CancellationToken::new();
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts);
    let internal_auth_state = InternalMcpAuthState {
        verifier: internal_token_verifier,
    };
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(TimeseriesMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_stateful_mode(false)
            .with_json_response(true)
            .with_cancellation_token(ct.child_token()),
    );
    let task_extension = Arc::new(TaskExtensionAdapter::new(
        Arc::new(TimeseriesTaskExtension::new(state.clone())),
        ServerDiscovery::new(
            std::collections::BTreeMap::from([
                ("tools".to_owned(), json!({})),
                ("resources".to_owned(), json!({})),
            ]),
            TaskExtensionImplementation {
                name: "timeseries".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some(
                "Timeseries forecasting with durable asynchronous tasks and immutable Rerun artifacts."
                    .to_owned(),
            ),
        ),
    ));
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            task_extension,
            task_extension_middleware::<TimeseriesTaskExtension>,
        ))
        .layer(middleware::from_fn_with_state(
            internal_auth_state,
            authenticate_internal_mcp,
        ));
    let server_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state.clone())
        .nest("/mcp", mcp_router);
    let router = Router::new()
        .nest(public_endpoint.mount_path(), server_router)
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(
        service = "veoveo-timeseries-mcp",
        address = %addr,
        mcp_path = public_endpoint.path("mcp"),
        public_url = public_endpoint.public_url(),
        "listening"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}
