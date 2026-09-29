//! DuckDB MCP server.
//!
//! MCP surface:
//!   tool `query(db, sql, ...)` — read-only SQL, direct or official Tasks
//!   tool `execute(db, sql, ...)` — arbitrary DDL/DML, direct or official Tasks
//!   tool `ingest(db, table, source, mode)` — final task-extension source loading
//!   tool `export(db, selection, format)` — final task-extension artifact export
//!   resource `duckdb://dbs` — databases visible to the caller
//!   template `duckdb://db/{db_id}` — schema summary for one database
//!   template `duckdb://artifact/{artifact_id}` — immutable export artifact bytes
//!   template `duckdb://usage/task/{task_id}` — task usage rows
use veoveo_duckdb_mcp::contract::DuckDbTaskKind;

use std::{
    collections::BTreeSet,
    net::SocketAddr,
    num::{NonZeroU32, NonZeroU64},
    sync::{Arc, LazyLock},
    time::Duration,
};
use veoveo_types::TaskTypeDefinition;

use axum::{Router, middleware, routing::get};
use chrono::{TimeDelta, Utc};
use clap::Parser;
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, CancelTaskParams, ContentBlock,
        GetTaskParams, GetTaskResult, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ServerConfig,
        SubscriptionFilter, UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
    transport::streamable_http_server::StreamableHttpService,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_duckdb_mcp::{
    artifacts::ArtifactRepository,
    contract::{
        DuckDbExecuteOutput, DuckDbExecuteRequest, DuckDbExportOutput, DuckDbExportRequest,
        DuckDbIngestOutput, DuckDbIngestRequest, DuckDbQueryOutput, DuckDbQueryRequest,
    },
    engine::{self, EngineSettings, TrustedExtension},
    state::TaskOwner,
    uris,
};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability, Page, ServerSlug,
    TelemetryGuard, TokenIssuer, init_server_telemetry, paginate, public_allowed_hosts,
};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, RecoveryClass, TaskError, TaskFailure, TaskRetentionPin,
    TaskRuntime, TaskRuntimeConfig, TaskSnapshot, TaskTransition,
};
use veoveo_types::TaskId;

#[path = "server/admin.rs"]
mod admin;
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
#[path = "server/resources.rs"]
mod resources;
#[path = "server/setup.rs"]
mod setup;
#[path = "server/sql_ops.rs"]
mod sql_ops;
#[path = "server/task_extension.rs"]
mod task_extension;

use app_state::{AppState, Caps, ServerDirs, update_task};
use config::Args;
use host::validate_host;
use internal_auth::{InternalMcpAuthState, authenticate_internal_mcp};
use ownership::{
    identity_from_runtime, internal_caller, internal_identity, runtime_owner,
    task_owner_from_identity, task_owner_from_runtime,
};
use sql_ops::ArtifactWriteContext;
use task_extension::DuckdbTaskService;

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3000;
const MCP_TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);
const SERVER_SLUG: &str = "duckdb";
const LIST_PAGE_SIZE: usize = 100;

use setup::SERVER_DOCS;

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn direct_call_owner(identity: &veoveo_mcp_contract::GatewayInternalIdentity) -> TaskOwner {
    task_owner_from_identity(&format!("call-{}", uuid::Uuid::now_v7()), identity)
}

#[derive(Clone)]
struct DuckdbMcp {
    state: Arc<AppState>,
    task_service: DuckdbTaskService,
    #[allow(dead_code)]
    tool_router: ToolRouter<DuckdbMcp>,
}

#[tool_router]
impl DuckdbMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            task_service: DuckdbTaskService::new(state.clone()),
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Query a DuckDB database",
        description = "Run one read-only SQL statement against a database you own. DuckDB Spatial is preloaded by the server. Read-only is enforced by the connection, and SQL cannot touch files, the network, additional extensions, or engine settings. Inline output is capped; pass output = {mode: \"artifact\", format: \"parquet\"} for large results, which returns one duckdb://artifact/{artifact_id} link. To query someone else's data, have them export a snapshot as an artifact and grant you access, then ingest it here with an artifact:// source.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<DuckDbQueryOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn query(
        &self,
        Parameters(args): Parameters<DuckDbQueryRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let caller = internal_caller(&context)?;
        let identity = internal_identity(&context)?;
        let owner = direct_call_owner(&identity);
        let writer = ArtifactWriteContext::Caller(Box::new(caller));
        let output = sql_ops::query_op(&self.state, &writer, &identity, &owner, args).await?;
        outputs::query_result(&output)
    }

    #[tool(
        title = "Execute SQL on a DuckDB database",
        description = "Run DDL/DML SQL on a database owned by the caller, creating it when create_if_missing is set. DuckDB Spatial is preloaded by the server. Writes serialize per database. SQL cannot touch files, the network, additional extensions, or engine settings.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<DuckDbExecuteOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn execute(
        &self,
        Parameters(args): Parameters<DuckDbExecuteRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = internal_identity(&context)?;
        let output = sql_ops::execute_op(&self.state, &identity, args).await?;
        outputs::execute_result(&output)
    }

    #[tool(
        title = "Ingest data into a DuckDB table",
        description = "Load a source into one table: inline CSV, an allowlisted HTTPS URI, or an artifact:// reference you can read. The server fetches the source itself, so SQL never reaches the network. Run as an MCP Task; the completed task carries the result.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<DuckDbIngestOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn ingest(
        &self,
        Parameters(_args): Parameters<DuckDbIngestRequest>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "ingest requires final task-extension invocation",
            None,
        ))
    }

    #[tool(
        title = "Export DuckDB data to an artifact",
        description = "Export a table, a read-only SQL result, or a snapshot of a database you own as one duckdb://artifact/{artifact_id} artifact (parquet, csv, or duck_db snapshot). Run as an MCP Task; the completed task carries the artifact link.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<DuckDbExportOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn export(
        &self,
        Parameters(_args): Parameters<DuckDbExportRequest>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "export requires final task-extension invocation",
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", content = "request", rename_all = "snake_case")]
enum TaskArgs {
    Query(DuckDbQueryRequest),
    Execute(DuckDbExecuteRequest),
    Ingest(DuckDbIngestRequest),
    Export(DuckDbExportRequest),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DuckdbTaskRequest {
    args: TaskArgs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact_write_capability: Option<IssuedArtifactWriteCapability>,
}

fn parse_task_args(name: &str, arguments: Value) -> Result<TaskArgs, String> {
    let invalid = |error: serde_json::Error| format!("invalid {name} arguments: {error}");
    match name {
        "query" => Ok(TaskArgs::Query(
            serde_json::from_value(arguments).map_err(invalid)?,
        )),
        "execute" => Ok(TaskArgs::Execute(
            serde_json::from_value(arguments).map_err(invalid)?,
        )),
        "ingest" => Ok(TaskArgs::Ingest(
            serde_json::from_value(arguments).map_err(invalid)?,
        )),
        "export" => Ok(TaskArgs::Export(
            serde_json::from_value(arguments).map_err(invalid)?,
        )),
        _ => Err(format!("unknown DuckDB tool {name:?}")),
    }
}

#[tool_handler]
impl ServerHandler for DuckdbMcp {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }

    fn get_info(&self) -> ServerConfig {
        setup::SERVER_SETUP.server_config().clone()
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

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_task_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        veoveo_task_runtime::listen_durable_subscriptions(&self.task_service, context, None, None)
            .await
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools = tools
            .into_iter()
            .map(|tool| {
                veoveo_mcp_apps_extension::link_tool_to_app(
                    tool,
                    uris::WORKBENCH_APP_URI,
                    &[
                        veoveo_mcp_apps_extension::UiVisibility::Model,
                        veoveo_mcp_apps_extension::UiVisibility::App,
                    ],
                )
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
        internal_identity(&context)?;
        let mut resources = setup::SERVER_SETUP
            .resources()
            .iter()
            .map(|r| r.descriptor().clone())
            .collect::<Vec<_>>();
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
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
        let page = mcp_page(
            setup::SERVER_SETUP
                .resource_templates()
                .iter()
                .map(|r| r.descriptor().clone())
                .collect::<Vec<_>>(),
            request.as_ref(),
        )?;
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
        resources::read(&self.state, &request.uri, &context)
            .await
            .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }
}

fn task_recovery_class(args: &TaskArgs) -> RecoveryClass {
    match args {
        TaskArgs::Query(_) | TaskArgs::Export(_) => RecoveryClass::Resume,
        TaskArgs::Execute(_) | TaskArgs::Ingest(_) => RecoveryClass::InterruptedIndeterminate,
    }
}

fn task_needs_artifact_capability(args: &TaskArgs) -> bool {
    matches!(args, TaskArgs::Query(_) | TaskArgs::Export(_))
}

async fn start_duckdb_task(
    state: Arc<AppState>,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: veoveo_mcp_contract::PlaneCaller,
    args: TaskArgs,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let task_id = TaskId::new();
    let artifact_write_capability = if task_needs_artifact_capability(&args) {
        Some(
            state
                .artifacts
                .issue_write_capability(
                    &caller,
                    &IssueArtifactWriteCapabilityRequest {
                        required_data_labels: Default::default(),
                        task_id: task_id.to_string(),
                        expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                        max_artifact_count: NonZeroU32::new(1).expect("one artifact is non-zero"),
                        max_total_bytes: NonZeroU64::new(state.max_artifact_bytes)
                            .ok_or_else(|| "max artifact bytes must be non-zero".to_owned())?,
                    },
                )
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let recovery_class = task_recovery_class(&args);
    let request = DuckdbTaskRequest {
        args,
        artifact_write_capability,
    };
    let created = state
        .tasks
        .create(DurableCreateTask {
            task_id,
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type: request.args.task_type(),
            request: serde_json::to_value(&request).map_err(|error| error.to_string())?,
            recovery_class,
            idempotency_key: None,
            ttl_ms: Some(MCP_TASK_TTL_MS),
            poll_interval_ms: Some(MCP_TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|error| error.to_string())?;
    schedule_duckdb_task(state, created.snapshot, request, identity, Some(caller))
        .await
        .map_err(|error| error.to_string())
}

impl TaskArgs {
    fn task_type(&self) -> veoveo_types::TaskTypeName {
        match self {
            Self::Query(_) => DuckDbTaskKind::Query.name(),
            Self::Execute(_) => DuckDbTaskKind::Execute.name(),
            Self::Ingest(_) => DuckDbTaskKind::Ingest.name(),
            Self::Export(_) => DuckDbTaskKind::Export.name(),
        }
    }
}

async fn schedule_duckdb_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: DuckdbTaskRequest,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: Option<veoveo_mcp_contract::PlaneCaller>,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state.tasks.claim(&task_id, TASK_LEASE_DURATION).await?;
    let owner = task_owner_from_runtime(&task_id, &snapshot.owner).map_err(anyhow::Error::msg)?;
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id.clone(),
        identity,
        owner,
        request,
        caller,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn resume_duckdb_task(state: Arc<AppState>, snapshot: TaskSnapshot) -> anyhow::Result<()> {
    let request: DuckdbTaskRequest = serde_json::from_value(snapshot.request.clone())?;
    if !matches!(&request.args, TaskArgs::Query(_) | TaskArgs::Export(_)) {
        anyhow::bail!("mutation task cannot be resumed");
    }
    let identity = identity_from_runtime(&snapshot.owner).map_err(anyhow::Error::msg)?;
    schedule_duckdb_task(state, snapshot, request, identity, None)
        .await
        .map(|_| ())
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
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    owner: TaskOwner,
    request: DuckdbTaskRequest,
    caller: Option<veoveo_mcp_contract::PlaneCaller>,
    cancellation: CancellationToken,
) {
    let work = run_task_inner(
        state.clone(),
        task_id.clone(),
        identity,
        owner,
        request,
        caller,
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
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    owner: TaskOwner,
    request: DuckdbTaskRequest,
    caller: Option<veoveo_mcp_contract::PlaneCaller>,
    cancellation: CancellationToken,
) {
    macro_rules! fail {
        ($msg:expr) => {{
            let msg: String = $msg;
            tracing::warn!(task_id, "duckdb task failed: {msg}");
            complete_tool_error(&state, &task_id, msg).await;
            return;
        }};
    }
    update_task(
        &state,
        &task_id,
        TaskTransition::Running {
            message: "running DuckDB operation".to_owned(),
            progress: 0.1,
        },
    )
    .await;
    let artifact_write_capability = request.artifact_write_capability;
    let result = match request.args {
        TaskArgs::Query(request) => {
            let writer =
                match task_artifact_writer(artifact_write_capability.as_ref(), &task_id, "query") {
                    Ok(writer) => writer,
                    Err(error) => fail!(error),
                };
            match sql_ops::query_op(&state, &writer, &identity, &owner, request).await {
                Ok(output) => {
                    if let Err(error) = outputs::record_op_usage(
                        &state,
                        &task_id,
                        "query",
                        output.row_count,
                        json!({ "db": output_db_meta(&output) }),
                    )
                    .await
                    {
                        fail!(format!("usage write failed: {error}"));
                    }
                    outputs::query_result(&output)
                }
                Err(err) => fail!(format!("query failed: {}", err.message)),
            }
        }
        TaskArgs::Execute(request) => match sql_ops::execute_op(&state, &identity, request).await {
            Ok(output) => {
                if let Err(error) = outputs::record_op_usage(
                    &state,
                    &task_id,
                    "execute",
                    output.rows_changed,
                    json!({ "db": output.db.as_str(), "statements": output.statements }),
                )
                .await
                {
                    fail!(format!("usage write failed: {error}"));
                }
                outputs::execute_result(&output)
            }
            Err(err) => fail!(format!("execute failed: {}", err.message)),
        },
        TaskArgs::Ingest(request) => {
            let caller = match caller.as_ref() {
                Some(caller) => caller,
                None => fail!("interrupted ingest cannot be replayed".to_owned()),
            };
            match sql_ops::ingest_op(&state, caller, &identity, request).await {
                Ok(output) => {
                    if let Err(error) = outputs::record_op_usage(
                        &state,
                        &task_id,
                        "ingest",
                        output.rows_ingested,
                        json!({ "db": output.db.as_str(), "table": output.table }),
                    )
                    .await
                    {
                        fail!(format!("usage write failed: {error}"));
                    }
                    outputs::ingest_result(&output)
                }
                Err(err) => fail!(format!("ingest failed: {}", err.message)),
            }
        }
        TaskArgs::Export(request) => {
            let writer = match task_artifact_writer(
                artifact_write_capability.as_ref(),
                &task_id,
                "export",
            ) {
                Ok(writer) => writer,
                Err(error) => fail!(error),
            };
            match sql_ops::export_op(&state, &writer, &identity, &owner, request).await {
                Ok(output) => {
                    if let Err(error) = outputs::record_op_usage(
                        &state,
                        &task_id,
                        "export",
                        output.rows_exported,
                        json!({ "db": output.db.as_str(), "artifact": output.artifact.artifact_id() }),
                    )
                    .await
                    {
                        fail!(format!("usage write failed: {error}"));
                    }
                    outputs::export_result(&output)
                }
                Err(err) => fail!(format!("export failed: {}", err.message)),
            }
        }
    };
    let result = match result {
        Ok(result) => result,
        Err(err) => fail!(format!("result assembly failed: {}", err.message)),
    };
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    let payload = match serde_json::to_value(&result) {
        Ok(payload) => payload,
        Err(error) => fail!(format!("serializing result failed: {error}")),
    };
    update_task(
        &state,
        &task_id,
        TaskTransition::Succeeded {
            message: "DuckDB operation completed".to_owned(),
            result: payload,
        },
    )
    .await;
}

fn task_artifact_writer(
    capability: Option<&IssuedArtifactWriteCapability>,
    task_id: &str,
    operation: &str,
) -> Result<ArtifactWriteContext, String> {
    let capability = capability
        .cloned()
        .ok_or_else(|| "task did not reserve artifact write capability".to_owned())?;
    let idempotency_key = veoveo_mcp_contract::ArtifactWriteIdempotencyKey::new(format!(
        "duckdb:{task_id}:{operation}"
    ))
    .map_err(|error| error.to_string())?;
    Ok(ArtifactWriteContext::Capability {
        capability,
        idempotency_key,
    })
}

fn output_db_meta(output: &DuckDbQueryOutput) -> Value {
    output
        .artifact
        .as_ref()
        .map(|artifact| json!({ "artifact": artifact.artifact_id() }))
        .unwrap_or_else(|| json!({ "inline_rows": output.rows.len() }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    LazyLock::force(&setup::SERVER_SETUP);
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-duckdb-mcp", "info,veoveo_duckdb_mcp=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    for dir in [&args.database_dir, &args.exchange_dir, &args.spill_dir] {
        std::fs::create_dir_all(dir)?;
    }
    let engine_settings = EngineSettings {
        memory_limit: args.engine_memory_limit.clone(),
        threads: args.engine_threads,
        spill_dir: args.spill_dir.clone(),
        trusted_extensions: vec![TrustedExtension::new(
            "spatial",
            args.spatial_extension.clone(),
        )?],
        spatial_axis_policy: engine::SpatialAxisPolicy::Native,
    };
    engine::verify_spatial(&engine_settings)?;
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
    let mut source_policy =
        veoveo_duckdb_runtime::HttpsSourcePolicy::new(args.allow_source_hosts.clone());
    source_policy.max_bytes = args.max_source_bytes;
    let state = Arc::new(AppState::new(
        tasks,
        artifacts,
        engine_settings,
        ServerDirs {
            database_dir: args.database_dir.clone(),
            exchange_dir: args.exchange_dir.clone(),
        },
        Caps {
            max_inline_rows: args.max_inline_rows,
            max_inline_bytes: args.max_inline_bytes,
            default_timeout_ms: args.default_timeout_ms,
            max_timeout_ms: args.max_timeout_ms,
        },
        source_policy,
        args.max_artifact_bytes,
    ));
    for snapshot in recovery.resumable {
        if let Err(error) = resume_duckdb_task(state.clone(), snapshot).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(task_id, "another replica claimed recovered DuckDB task");
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
            move || Ok(DuckdbMcp::new(state.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(ct.child_token()),
    );
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn_with_state(
            internal_auth_state.clone(),
            authenticate_internal_mcp,
        ));
    let admin_router = admin::router().layer(middleware::from_fn_with_state(
        internal_auth_state,
        authenticate_internal_mcp,
    ));
    let server_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state.clone())
        .nest("/admin", admin_router)
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
        service = "veoveo-duckdb-mcp",
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
mod well_known_tests {
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::SERVER_DOCS;

    #[test]
    fn discovery_uses_collection_roots_and_typed_usage_templates() {
        use veoveo_duckdb_mcp::{
            contract::{DuckDbTaskUsageUri, DuckDbUsageIndexUri},
            uris,
        };
        let resources = super::setup::SERVER_SETUP
            .resources()
            .iter()
            .map(|r| r.descriptor().clone())
            .collect::<Vec<_>>();
        assert!(
            resources
                .iter()
                .any(|resource| resource.uri == DuckDbUsageIndexUri::ROOT)
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource.uri == uris::DBS_ROOT_URI)
        );
        assert!(resources.iter().all(|resource| {
            !resource.uri.starts_with("duckdb://usage/task/")
                && !resource.uri.starts_with("duckdb://db/")
        }));
        let templates = super::setup::SERVER_SETUP
            .resource_templates()
            .iter()
            .map(|r| r.descriptor().clone())
            .collect::<Vec<_>>();
        for uri in [DuckDbUsageIndexUri::TEMPLATE, DuckDbTaskUsageUri::TEMPLATE] {
            assert!(
                templates
                    .iter()
                    .any(|template| template.uri_template == uri)
            );
        }
    }

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "duckdb");
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
        assert_eq!(declaration.server, "duckdb");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        for item in &declaration.compliance {
            if item.status == ComplianceStatus::Pending {
                assert!(item.note.is_some(), "pending items must state a reason");
            }
        }
    }

    #[test]
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}

#[cfg(test)]
mod task_tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!DuckdbMcp::tool_router().list_all().is_empty());
    }

    fn task_args(name: &str, arguments: Value) -> TaskArgs {
        parse_task_args(name, arguments).unwrap()
    }

    #[test]
    fn only_read_operations_are_resumable() {
        let query = task_args("query", json!({"db": "analytics", "sql": "select 1"}));
        let export = task_args(
            "export",
            json!({
                "db": "analytics",
                "selection": {"kind": "database"},
                "format": "duck_db"
            }),
        );
        let execute = task_args(
            "execute",
            json!({"db": "analytics", "sql": "create table rows(value int)"}),
        );
        let ingest = task_args(
            "ingest",
            json!({
                "db": "analytics",
                "table": "rows",
                "source": {"kind": "inline_csv", "csv": "value\n1\n"},
                "mode": "append"
            }),
        );

        assert_eq!(task_recovery_class(&query), RecoveryClass::Resume);
        assert_eq!(task_recovery_class(&export), RecoveryClass::Resume);
        assert_eq!(
            task_recovery_class(&execute),
            RecoveryClass::InterruptedIndeterminate
        );
        assert_eq!(
            task_recovery_class(&ingest),
            RecoveryClass::InterruptedIndeterminate
        );
    }
}
