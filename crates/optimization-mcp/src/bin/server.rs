//! Optimization MCP server.
//!
//! MCP surface:
//!   tool `plan(input, objective?, artifacts?)` — task-required
//!   template `optimization://artifact/{sha256}` — immutable plan artifact bytes
//!   template `optimization://usage/task/{task_id}` — task usage rows

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    Router,
    extract::{Path as AxumPath, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    middleware,
    response::IntoResponse,
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use clap::Parser;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, CancelTaskParams, CancelTaskResult,
        CreateTaskResult, GetTaskParams, GetTaskPayloadParams, GetTaskPayloadResult, GetTaskResult,
        ListResourceTemplatesResult, ListResourcesResult, ListTasksResult, ListToolsResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, Resource,
        ResourceContents, ResourceTemplate, ServerCapabilities, ServerInfo, Task, TaskStatus,
        TasksCapability,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, InternalTokenSecret, Page,
    PlanOutput, PlanRequest, ServerResourceUris, ServerSlug, TaskPayloadState, TaskStore,
    TelemetryGuard, TokenIssuer, UsageReport, init_server_telemetry, is_sha256, now_iso, paginate,
    public_allowed_hosts, related_task_meta,
};
use veoveo_optimization_mcp::{
    artifacts::{ArtifactRepository, S3ArtifactConfig},
    planning::{RRD_MIME_TYPE, run_plan},
    state::DuckdbState,
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

use app_state::{AppState, update_task};
use config::{Args, ArtifactStoreBackend};
use host::validate_host;
use internal_auth::{
    InternalMcpAuthState, authenticate_internal_mcp, verify_internal_authorization,
};
use outputs::plan_result;
use ownership::{
    artifact_owned_by, internal_identity, optional_task_owner, require_task_owner,
    task_owner_allows, task_owner_from_identity,
};

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3000;
const SERVER_SLUG: &str = "optimization";
const LIST_PAGE_SIZE: usize = 100;

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
        description = "Solve a high-level task-option planning problem for one or many agents. Inputs are typed planning objects or DuckDB-readable option rows using the shared DuckDbSource contract. Returns a structured plan plus optional optimization://artifact/{sha256} DuckDB and Rerun RRD artifacts. Must be invoked as an MCP task; read tasks/result for the final output and optimization://usage/task/{task_id} for usage.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<PlanOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        execution(task_support = "required")
    )]
    async fn plan(
        &self,
        Parameters(_args): Parameters<PlanRequest>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "plan requires task-based invocation",
            None,
        ))
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
impl ServerHandler for OptimizationMcp {
    fn get_info(&self) -> ServerInfo {
        let caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_tasks_with(TasksCapability::server_default())
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = caps;
        info.server_info =
            rmcp::model::Implementation::new("optimization", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Optimization planning server. Workflow: call `plan` as a task with typed agents, \
             tasks, options, and constraints, or with typed agents/tasks plus DuckDbSource option \
             rows; read tasks/get until completed; then read tasks/result for the structured plan \
             and optimization://artifact/{sha256} outputs."
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

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        let identity = internal_identity(&context)?;
        if request.name != "plan" {
            return Err(McpError::method_not_found::<
                rmcp::model::CallToolRequestMethod,
            >());
        }
        let args: PlanRequest =
            serde_json::from_value(Value::Object(request.arguments.clone().unwrap_or_default()))
                .map_err(|err| {
                    McpError::invalid_params(format!("invalid plan arguments: {err}"), None)
                })?;
        let progress_token = context.meta.get_progress_token();
        let ttl = request.task.as_ref().and_then(|task| task.ttl);
        let task_id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        let mut task = Task::new(task_id.clone(), TaskStatus::Working, now.clone(), now)
            .with_status_message("accepted; building plan model")
            .with_poll_interval(MCP_TASK_POLL_INTERVAL_MS);
        task.ttl = ttl;

        self.state.tasks.insert(task.clone(), None).await;
        let owner = task_owner_from_identity(&task_id, &identity);
        self.state
            .durable
            .record_task_owner(&owner)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;
        self.state
            .task_owners
            .write()
            .await
            .insert(task_id.clone(), owner.clone());
        if let Err(err) = self.state.durable.record_task(&task, None, None) {
            tracing::warn!(task_id, "failed to persist task creation: {err}");
        }

        let join = tokio::spawn(run_task(
            self.state.clone(),
            context.peer.clone(),
            task_id.clone(),
            owner,
            args,
            progress_token,
        ));
        self.state.tasks.set_join(&task_id, join).await;
        Ok(CreateTaskResult::new(task).with_meta(related_task_meta(task_id)))
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let identity = internal_identity(&context)?;
        let owners = self.state.task_owners.read().await;
        let tasks = self
            .state
            .tasks
            .list()
            .await
            .into_iter()
            .filter(|task| {
                owners
                    .get(&task.task_id)
                    .map(|owner| task_owner_allows(owner, &identity))
                    .unwrap_or(false)
            })
            .collect();
        let page = mcp_page(tasks, request.as_ref())?;
        let mut result = ListTasksResult::new(page.items);
        result.next_cursor = page.next_cursor;
        Ok(result)
    }

    async fn get_task_info(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        require_task_owner(&self.state, &context, &request.task_id).await?;
        let task = self
            .state
            .tasks
            .get(&request.task_id)
            .await
            .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
        Ok(GetTaskResult::new(task))
    }

    async fn get_task_result(
        &self,
        request: GetTaskPayloadParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        require_task_owner(&self.state, &context, &request.task_id).await?;
        match self.state.tasks.payload_state(&request.task_id).await {
            TaskPayloadState::Completed(payload) => Ok(GetTaskPayloadResult::new(payload)),
            TaskPayloadState::Failed(error) => Err(McpError::internal_error(error, None)),
            TaskPayloadState::Cancelled => {
                Err(McpError::invalid_request("task was cancelled", None))
            }
            TaskPayloadState::Running => Err(McpError::invalid_request(
                "task is still running; read tasks/get until completed",
                None,
            )),
            TaskPayloadState::Unknown => Err(McpError::invalid_params("unknown task id", None)),
        }
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        require_task_owner(&self.state, &context, &request.task_id).await?;
        let task = self
            .state
            .tasks
            .cancel(&request.task_id)
            .await
            .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
        if let Err(err) = self.state.durable.record_task(&task, None, None) {
            tracing::warn!(
                task_id = request.task_id,
                "failed to persist task cancellation: {err}"
            );
        }
        Ok(CancelTaskResult::new(task))
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
        for artifact in self
            .state
            .durable
            .list_artifacts()
            .map_err(|err| McpError::internal_error(err.to_string(), None))?
        {
            if artifact_owned_by(&self.state, &artifact.sha256, &identity).is_err() {
                continue;
            }
            resources.push(
                Resource::new(artifact.artifact_uri.clone(), artifact.sha256.clone())
                    .with_title("Optimization plan artifact")
                    .with_description("Immutable plan artifact.")
                    .with_mime_type(
                        artifact
                            .mime_type
                            .clone()
                            .unwrap_or_else(|| RRD_MIME_TYPE.to_string()),
                    ),
            );
        }
        for task_id in self
            .state
            .durable
            .usage_task_ids()
            .map_err(|err| McpError::internal_error(err.to_string(), None))?
        {
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
                .with_description("Server-owned immutable plan artifact, addressed by sha256."),
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
                .durable
                .usage_task_ids()
                .map_err(|err| McpError::internal_error(err.to_string(), None))?
            {
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
                .durable
                .usage_records(task_id)
                .map_err(|err| McpError::internal_error(err.to_string(), None))?;
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
        if let Some(sha256) = uris::parse_artifact_uri(uri) {
            let artifact = self
                .state
                .artifacts
                .get(sha256)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown artifact '{sha256}'"), None)
                })?;
            artifact_owned_by(&self.state, sha256, &identity)?;
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

async fn run_task(
    state: Arc<AppState>,
    peer: rmcp::service::Peer<RoleServer>,
    task_id: String,
    owner: veoveo_optimization_mcp::state::TaskOwner,
    args: PlanRequest,
    progress_token: Option<rmcp::model::ProgressToken>,
) {
    macro_rules! fail {
        ($msg:expr) => {{
            let msg: String = $msg;
            tracing::warn!(task_id, "optimization task failed: {msg}");
            update_task(
                &state,
                &peer,
                &task_id,
                TaskStatus::Failed,
                msg.clone(),
                None,
                Some(msg),
            )
            .await;
            return;
        }};
    }
    veoveo_mcp_contract::notify_progress(&peer, &progress_token, 0.1, "building plan model").await;
    let run = match tokio::task::spawn_blocking({
        let task_id = task_id.clone();
        let args = args.clone();
        move || run_plan(&task_id, &args)
    })
    .await
    {
        Ok(Ok(run)) => run,
        Ok(Err(err)) => fail!(format!("plan failed: {err}")),
        Err(err) => fail!(format!("plan worker failed: {err}")),
    };
    veoveo_mcp_contract::notify_progress(&peer, &progress_token, 0.8, "writing artifacts").await;
    let result = match plan_result(&state, &task_id, &owner, run).await {
        Ok(result) => result,
        Err(err) => fail!(format!("artifact write failed: {err}")),
    };
    veoveo_mcp_contract::notify_progress(&peer, &progress_token, 1.0, "completed").await;
    update_task(
        &state,
        &peer,
        &task_id,
        TaskStatus::Completed,
        "completed; plan available",
        serde_json::to_value(&result).ok(),
        None,
    )
    .await;
}

async fn artifact_download(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AxumPath(sha256): AxumPath<String>,
) -> impl IntoResponse {
    if !is_sha256(&sha256) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let identity = match verify_internal_authorization(&state.internal_token_verifier, &headers) {
        Ok(identity) => identity,
        Err(message) => {
            tracing::warn!(
                artifact_sha256 = sha256,
                "rejected optimization artifact download: {message}"
            );
            return (StatusCode::UNAUTHORIZED, "gateway authorization required").into_response();
        }
    };
    if let Err(err) = artifact_owned_by(&state, &sha256, &identity) {
        tracing::warn!(
            artifact_sha256 = sha256,
            "rejected optimization artifact download: {err}"
        );
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let artifact = match state.artifacts.get(&sha256).await {
        Ok(Some(artifact)) => artifact,
        Ok(None) => return (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(err) => {
            tracing::warn!(artifact_sha256 = sha256, "artifact download failed: {err}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "artifact unavailable").into_response();
        }
    };

    let mut headers = HeaderMap::new();
    let mime = artifact
        .metadata
        .mime_type
        .as_deref()
        .unwrap_or(RRD_MIME_TYPE);
    if let Ok(value) = HeaderValue::from_str(mime) {
        headers.insert(CONTENT_TYPE, value);
    }
    let filename = artifact
        .metadata
        .filename
        .as_deref()
        .unwrap_or("plan.artifact")
        .replace(['"', '\r', '\n'], "_");
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        headers.insert(CONTENT_DISPOSITION, value);
    }
    (StatusCode::OK, headers, artifact.bytes).into_response()
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
    let durable = DuckdbState::open(&args.state_db)?;
    let internal_token_verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        InternalTokenSecret::new(args.internal_token_secret.clone())?,
    );
    let artifacts = match args.artifact_store {
        ArtifactStoreBackend::S3Compatible => ArtifactRepository::new_s3_compatible(
            S3ArtifactConfig {
                endpoint: args.artifact_endpoint.clone(),
                bucket: args.artifact_bucket.clone(),
                region: args.artifact_region.clone(),
                allow_http: args.artifact_allow_http,
            },
            durable.clone(),
            ServerResourceUris::new(SERVER_SLUG),
            public_endpoint.public_url().to_string(),
        )?,
        ArtifactStoreBackend::Memory => ArtifactRepository::new_in_memory(
            durable.clone(),
            ServerResourceUris::new(SERVER_SLUG),
            public_endpoint.public_url().to_string(),
        ),
    };
    let tasks = TaskStore::new();
    for persisted in durable.load_tasks()? {
        tasks
            .insert_record(
                persisted.task,
                persisted.payload,
                persisted.error,
                None,
                None,
            )
            .await;
    }
    let task_owners = durable
        .load_task_owners()?
        .into_iter()
        .map(|owner| (owner.task_id.clone(), owner))
        .collect::<HashMap<_, _>>();
    let state = Arc::new(AppState {
        tasks,
        durable,
        artifacts,
        internal_token_verifier: internal_token_verifier.clone(),
        task_owners: RwLock::new(task_owners),
    });

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
            .with_cancellation_token(ct.child_token()),
    );
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            internal_auth_state,
            authenticate_internal_mcp,
        ));
    let server_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/artifacts/{sha256}", get(artifact_download))
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
