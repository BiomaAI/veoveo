//! Optimization MCP server.
//!
//! MCP surface:
//!   tool `plan(input, objective?, artifacts?)` — task-required
//!   template `optimization://artifact/{artifact_id}` — immutable plan artifact bytes
//!   template `optimization://usage/task/{task_id}` — task usage rows

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
    tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_duckdb_runtime::HttpsSourcePolicy;
use veoveo_mcp_contract::tool;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability, Page, ServerSlug,
    TelemetryGuard, TokenIssuer, UsageKind, UsageRecord, UsageReport, init_server_telemetry,
    paginate, public_allowed_hosts,
};
use veoveo_mcp_task_extension::{
    Implementation as TaskExtensionImplementation, ServerDiscovery, TaskExtensionAdapter,
    task_extension_middleware,
};
use veoveo_optimization_mcp::{
    artifacts::ArtifactRepository,
    contract::{PlanOutput, PlanRequest},
    planning::{RRD_MIME_TYPE, run_plan},
    uris,
};
use veoveo_platform_store::{DomainUsageKind as StoreUsageKind, DomainUsageRecord};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, RecoveryClass, TaskFailure, TaskId, TaskPayloadState,
    TaskRetentionPin, TaskRuntime, TaskRuntimeConfig, TaskSnapshot, TaskTransition,
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
use outputs::plan_result;
use ownership::{
    internal_caller, internal_identity, optional_task_owner, require_task_owner, runtime_owner,
    task_owner_allows, task_owner_from_identity, task_owner_from_runtime,
};
use task_extension::OptimizationTaskExtension;

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3000;
const MCP_TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);
const SERVER_SLUG: &str = "optimization";
const LIST_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PlanTaskRequest {
    input: PlanRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact_write_capability: Option<IssuedArtifactWriteCapability>,
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Clone)]
struct OptimizationMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<OptimizationMcp>,
}

#[tool_router]
impl OptimizationMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Plan task options",
        description = "Solve a high-level task-option planning problem for one or many agents. Inputs are typed planning objects or DuckDB-readable option rows using the shared DuckDbSource contract. Returns a structured plan plus optional optimization://artifact/{artifact_id} DuckDB and Rerun RRD artifacts. Clients declaring the task extension receive a durable task; direct calls wait for the same durable execution.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<PlanOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn plan(
        &self,
        Parameters(args): Parameters<PlanRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = internal_identity(&context)?;
        let snapshot = start_plan_task(
            self.state.clone(),
            identity,
            internal_caller(&context)?,
            args,
            Some(TaskProgress {
                peer: context.peer.clone(),
                token: context.meta.get_progress_token(),
            }),
            BTreeSet::new(),
            self.state.max_artifact_bytes,
        )
        .await
        .map_err(|error| McpError::internal_error(error, None))?;
        match self
            .state
            .tasks
            .await_payload_state(&snapshot.task_id.to_string())
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?
        {
            TaskPayloadState::Completed(payload) => {
                serde_json::from_value(payload).map_err(|error| {
                    McpError::internal_error(format!("invalid durable plan result: {error}"), None)
                })
            }
            TaskPayloadState::Failed(error) => Err(McpError::internal_error(
                error.message,
                Some(json!({"code": error.code, "details": error.details})),
            )),
            TaskPayloadState::Cancelled => {
                Err(McpError::invalid_request("plan was cancelled", None))
            }
            TaskPayloadState::Running => Err(McpError::internal_error(
                "durable plan wait ended while still running",
                None,
            )),
            TaskPayloadState::Unknown => Err(McpError::internal_error(
                "durable plan disappeared before completion",
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

fn usage_record(task_id: &str, record: DomainUsageRecord) -> UsageRecord {
    UsageRecord {
        task_id: task_id.to_owned(),
        source_id: record.source_id,
        provider_job_id: record.provider_job_id,
        model_id: record.model_id,
        kind: match record.kind {
            StoreUsageKind::Estimate => UsageKind::Estimate,
            StoreUsageKind::Actual => UsageKind::Actual,
        },
        quantity: record.quantity,
        unit: record.unit,
        amount: record.amount,
        currency: record.currency,
        recorded_at: record.recorded_at,
        metadata: serde_json::Value::Object(record.metadata.into_map().into_iter().collect()),
    }
}

#[tool_handler]
impl ServerHandler for OptimizationMcp {
    fn get_info(&self) -> ServerInfo {
        let caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = caps;
        info.server_info =
            rmcp::model::Implementation::new("optimization", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Optimization planning server. Workflow: call `plan` as a task with typed agents, \
             tasks, options, and constraints, or with typed agents/tasks plus DuckDbSource option \
             rows. Clients that declare the final task extension receive a durable task; direct \
             calls wait for that same task. Read optimization://artifact/{artifact_id} outputs."
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
            Resource::new(uris::USAGE_ROOT_URI, "usage")
                .with_title("Optimization usage ledger")
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
                .with_description("Usage rows for one optimization task.")
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
                .with_title("Optimization artifact")
                .with_description(
                    "Server-owned immutable plan artifact, addressed by occurrence id.",
                ),
            ResourceTemplate::new(uris::USAGE_TASK_TEMPLATE, "usage")
                .with_title("Optimization task usage")
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
            let records = self
                .state
                .tasks
                .platform_store()
                .domain_usage_for_task(
                    SERVER_SLUG,
                    task_id
                        .parse::<TaskId>()
                        .map_err(|err| McpError::invalid_params(err.to_string(), None))?,
                )
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
            if records.is_empty() {
                return Err(McpError::resource_not_found(
                    format!("unknown usage task '{task_id}'"),
                    None,
                ));
            }
            let report = UsageReport::new(task_id, uri).with_records(
                records
                    .into_iter()
                    .map(|record| usage_record(task_id, record))
                    .collect(),
            );
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

async fn start_plan_task(
    state: Arc<AppState>,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: veoveo_mcp_contract::PlaneCaller,
    input: PlanRequest,
    progress: Option<TaskProgress>,
    retention_pins: BTreeSet<TaskRetentionPin>,
    max_artifact_bytes: u64,
) -> Result<TaskSnapshot, String> {
    let task_id = TaskId::new();
    let artifact_count = u32::from(input.artifacts.duckdb) + u32::from(input.artifacts.rerun_rrd);
    let artifact_write_capability = if artifact_count == 0 {
        None
    } else {
        Some(
            state
                .artifacts
                .issue_write_capability(
                    &caller,
                    &IssueArtifactWriteCapabilityRequest {
                        task_id: task_id.to_string(),
                        expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                        max_artifact_count: NonZeroU32::new(artifact_count)
                            .ok_or_else(|| "artifact count must be non-zero".to_owned())?,
                        max_total_bytes: NonZeroU64::new(max_artifact_bytes)
                            .ok_or_else(|| "max artifact bytes must be non-zero".to_owned())?,
                    },
                )
                .await
                .map_err(|error| error.to_string())?,
        )
    };
    let request = PlanTaskRequest {
        input,
        artifact_write_capability,
    };
    let created = state
        .tasks
        .create(DurableCreateTask {
            task_id,
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type: "plan".to_owned(),
            request: serde_json::to_value(&request).map_err(|error| error.to_string())?,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(MCP_TASK_TTL_MS),
            poll_interval_ms: Some(MCP_TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|error| error.to_string())?;
    schedule_plan_task(
        state,
        created.snapshot,
        request,
        task_owner_from_identity(&task_id.to_string(), &identity),
        progress,
    )
    .await
}

async fn schedule_plan_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: PlanTaskRequest,
    owner: veoveo_optimization_mcp::state::TaskOwner,
    progress: Option<TaskProgress>,
) -> Result<TaskSnapshot, String> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state
        .tasks
        .claim(&task_id, TASK_LEASE_DURATION)
        .await
        .map_err(|error| error.to_string())?;
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
        .await
        .map_err(|error| error.to_string())?;
    Ok(claimed.snapshot)
}

async fn resume_plan_task(state: Arc<AppState>, snapshot: TaskSnapshot) -> Result<(), String> {
    let request: PlanTaskRequest =
        serde_json::from_value(snapshot.request.clone()).map_err(|error| error.to_string())?;
    let task_id = snapshot.task_id.to_string();
    let owner = task_owner_from_runtime(&task_id, &snapshot.owner)?;
    schedule_plan_task(state, snapshot, request, owner, None)
        .await
        .map(|_| ())
}

async fn report_progress(
    state: &AppState,
    task_id: &str,
    progress: &Option<TaskProgress>,
    value: f64,
    message: &str,
) {
    update_task(
        state,
        task_id,
        TaskTransition::Running {
            message: message.to_owned(),
            progress: value,
        },
    )
    .await;
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
    request: PlanTaskRequest,
    owner: veoveo_optimization_mcp::state::TaskOwner,
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
    request: PlanTaskRequest,
    owner: veoveo_optimization_mcp::state::TaskOwner,
    progress: Option<TaskProgress>,
    cancellation: CancellationToken,
) {
    macro_rules! fail {
        ($message:expr) => {{
            let message = $message.to_string();
            tracing::warn!(task_id, "optimization task failed: {message}");
            complete_tool_error(&state, &task_id, message).await;
            return;
        }};
    }
    report_progress(&state, &task_id, &progress, 0.1, "building plan model").await;
    let run = match tokio::task::spawn_blocking({
        let task_id = task_id.clone();
        let input = request.input.clone();
        let source_policy = state.source_policy.clone();
        move || run_plan(&task_id, &input, &source_policy)
    })
    .await
    {
        Ok(Ok(run)) => run,
        Ok(Err(error)) => fail!(format!("plan failed: {error}")),
        Err(error) => fail!(format!("plan worker failed: {error}")),
    };
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    report_progress(&state, &task_id, &progress, 0.8, "writing artifacts").await;
    let result = match plan_result(
        &state,
        request.artifact_write_capability.as_ref(),
        &task_id,
        &owner,
        run,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => fail!(format!("artifact write failed: {error}")),
    };
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    let payload = match serde_json::to_value(&result) {
        Ok(payload) => payload,
        Err(error) => fail!(format!("serializing plan result failed: {error}")),
    };
    update_task(
        &state,
        &task_id,
        TaskTransition::Succeeded {
            message: "completed; plan available".to_owned(),
            result: payload,
        },
    )
    .await;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard = init_server_telemetry(
        "veoveo-optimization-mcp",
        "info,veoveo_optimization_mcp=debug",
    )?;
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
        resume_plan_task(state.clone(), snapshot)
            .await
            .map_err(anyhow::Error::msg)?;
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
            move || Ok(OptimizationMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_stateful_mode(false)
            .with_json_response(true)
            .with_cancellation_token(ct.child_token()),
    );
    let task_extension = Arc::new(TaskExtensionAdapter::new(
        Arc::new(OptimizationTaskExtension::new(
            state.clone(),
            args.max_artifact_bytes,
        )),
        ServerDiscovery::new(
            std::collections::BTreeMap::from([
                ("tools".to_owned(), json!({})),
                ("resources".to_owned(), json!({})),
            ]),
            TaskExtensionImplementation {
                name: "optimization".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some(
                "Optimization planning with durable resumable tasks and governed artifacts."
                    .to_owned(),
            ),
        ),
    ));
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            task_extension,
            task_extension_middleware::<OptimizationTaskExtension>,
        ))
        .layer(middleware::from_fn_with_state(
            internal_auth_state,
            authenticate_internal_mcp,
        ));
    let server_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
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
        service = "veoveo-optimization-mcp",
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

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!OptimizationMcp::tool_router().list_all().is_empty());
    }
}
