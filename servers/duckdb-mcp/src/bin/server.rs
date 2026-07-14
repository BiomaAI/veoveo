//! DuckDB MCP server.
//!
//! MCP surface:
//!   tool `query(db, sql, ...)` — read-only SQL, direct or final task extension
//!   tool `execute(db, sql, ...)` — arbitrary DDL/DML, direct or final task extension
//!   tool `ingest(db, table, source, mode)` — final task-extension source loading
//!   tool `export(db, selection, format)` — final task-extension artifact export
//!   resource `duckdb://dbs` — databases visible to the caller
//!   template `duckdb://db/{db_id}` — schema summary for one database
//!   template `duckdb://artifact/{artifact_id}` — immutable export artifact bytes
//!   template `duckdb://usage/task/{task_id}` — task usage rows

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
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_duckdb_mcp::{
    artifacts::ArtifactRepository,
    contract::{
        DuckDbDatabaseId, DuckDbExecuteOutput, DuckDbExecuteRequest, DuckDbExportOutput,
        DuckDbExportRequest, DuckDbIngestOutput, DuckDbIngestRequest, DuckDbQueryOutput,
        DuckDbQueryRequest,
    },
    engine::{self, EngineSettings, FileExchange, TrustedExtension},
    state::TaskOwner,
    uris,
};
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
#[path = "server/sql_ops.rs"]
mod sql_ops;
#[path = "server/task_extension.rs"]
mod task_extension;

use app_state::{AppState, Caps, ServerDirs, update_task};
use config::Args;
use host::validate_host;
use internal_auth::{InternalMcpAuthState, authenticate_internal_mcp};
use outputs::usage_record;
use ownership::{
    databases_for_identity, identity_from_runtime, internal_caller, internal_identity,
    optional_task_owner, require_task_owner, resolve_readable_database, runtime_owner,
    task_owner_allows, task_owner_from_identity, task_owner_from_runtime,
};
use sql_ops::ArtifactWriteContext;
use task_extension::DuckdbTaskExtension;

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3000;
const MCP_TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);
const SERVER_SLUG: &str = "duckdb";
const LIST_PAGE_SIZE: usize = 100;

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn direct_call_owner(identity: &veoveo_mcp_contract::GatewayInternalIdentity) -> TaskOwner {
    task_owner_from_identity(&format!("call-{}", uuid::Uuid::now_v7()), identity)
}

#[derive(Clone)]
struct DuckdbMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<DuckdbMcp>,
}

#[tool_router]
impl DuckdbMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Query a DuckDB database",
        description = "Run one read-only SQL statement against a database you own. DuckDB Spatial is preloaded by the server. Read-only is enforced by the connection, and SQL cannot touch files, the network, additional extensions, or engine settings. Inline output is capped; pass output = {mode: \"artifact\", format: \"parquet\"} for large results, which returns one duckdb://artifact/{artifact_id} link. To query another principal's data, have them export a snapshot to the artifact plane and grant it, then ingest it here with an artifact:// source.",
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
        description = "Load a typed source into one table: inline CSV, allowlisted HTTPS URIs, or an authorized artifact:// reference. The server resolves sources itself and SQL never reaches the network. Invoke through the final task extension; the completed task carries the result.",
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
        description = "Export a table, a read-only SQL result, or a full owned-database snapshot to one immutable duckdb://artifact/{artifact_id} artifact (parquet, csv, or duck_db snapshot). Invoke through the final task extension; the completed task carries the artifact link.",
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
    fn get_info(&self) -> ServerInfo {
        let caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = caps;
        info.server_info = rmcp::model::Implementation::new("duckdb", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Hosted DuckDB server with owner-scoped mutable databases. Workflow: `execute` \
             with create_if_missing to create a database and tables; `ingest` (as a task) to \
             load data; `query` for read-only SQL with inline rows or artifact spill; `export` \
             (as a task) for parquet/csv/snapshot artifacts. Read duckdb://dbs for visible \
             databases and duckdb://db/{db_id} for a schema summary. SQL runs sandboxed: no \
             file, network, extension, or settings access."
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
            Resource::new(uris::DBS_ROOT_URI, "dbs")
                .with_title("DuckDB databases")
                .with_description("Databases visible to the caller.")
                .with_mime_type("application/json"),
            Resource::new(uris::USAGE_ROOT_URI, "usage")
                .with_title("DuckDB usage ledger")
                .with_description("Index of task usage resources.")
                .with_mime_type("application/json"),
        ];
        for database in databases_for_identity(&self.state, &identity)? {
            resources.push(
                Resource::new(
                    uris::db_uri(database.db_id.as_str()),
                    database.db_id.to_string(),
                )
                .with_title(format!("Database {}", database.db_id))
                .with_description("Schema summary for one database.")
                .with_mime_type("application/json"),
            );
        }
        // Artifacts live on the shared plane now; duckdb keeps no local artifact
        // index to enumerate here. They remain readable by their duckdb://artifact
        // URI through resources/read, which resolves against the plane.
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
                .with_description("Usage rows for one duckdb task.")
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
            ResourceTemplate::new(uris::DB_TEMPLATE, "db")
                .with_title("DuckDB database schema")
                .with_description("Tables and columns for one visible database.")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
                .with_title("DuckDB artifact")
                .with_description(
                    "Server-owned immutable export artifact, addressed by occurrence id.",
                ),
            ResourceTemplate::new(uris::USAGE_TASK_TEMPLATE, "usage")
                .with_title("DuckDB task usage")
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
        if uri == uris::DBS_ROOT_URI {
            let mut entries = Vec::new();
            for database in databases_for_identity(&self.state, &identity)? {
                entries.push(json!({
                    "db_id": database.db_id.as_str(),
                    "db_uri": uris::db_uri(database.db_id.as_str()),
                    "owned": database.principal_id == identity.principal.id,
                }));
            }
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(serde_json::to_string(&entries).unwrap_or_default(), uri)
                    .with_mime_type("application/json"),
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
        if let Some(db_id) = uris::parse_db_uri(uri) {
            let schema = database_schema_document(&self.state, &identity, db_id).await?;
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(serde_json::to_string(&schema).unwrap_or_default(), uri)
                    .with_mime_type("application/json"),
            ]));
        }
        if let Some(task_id) = uris::parse_usage_task_uri(uri) {
            require_task_owner(&self.state, &context, task_id).await?;
            let durable_task_id = task_id
                .parse::<TaskId>()
                .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
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
            // The plane enforces access with the caller's identity; a denial
            // surfaces as an error rather than None.
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
            if let Some(mime_type) = artifact.metadata.mime_type {
                content = content.with_mime_type(mime_type);
            }
            return Ok(ReadResourceResult::new(vec![content]));
        }
        Err(McpError::resource_not_found(
            format!("unknown resource uri: {uri}"),
            None,
        ))
    }
}

async fn database_schema_document(
    state: &Arc<AppState>,
    identity: &veoveo_mcp_contract::GatewayInternalIdentity,
    db_id: &str,
) -> Result<Value, McpError> {
    let db_id = DuckDbDatabaseId::new(db_id)
        .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
    let database = resolve_readable_database(state, identity, &db_id)?;
    let db_path = std::path::PathBuf::from(&database.file_path);
    if !db_path.exists() {
        return Ok(json!({ "db_id": db_id.as_str(), "tables": [] }));
    }
    let settings = state.engine.clone();
    let columns = tokio::task::spawn_blocking(move || -> anyhow::Result<engine::QueryRows> {
        let conn = engine::open_connection(&db_path, true, &[], &FileExchange::Denied, &settings)?;
        engine::run_query(
            &conn,
            "SELECT table_name, column_name, data_type FROM information_schema.columns \
             WHERE table_schema = 'main' ORDER BY table_name, ordinal_position",
            100_000,
            16 * 1024 * 1024,
        )
    })
    .await
    .map_err(|err| McpError::internal_error(err.to_string(), None))?
    .map_err(|err| McpError::internal_error(format!("reading schema failed: {err:#}"), None))?;

    let mut tables: Vec<Value> = Vec::new();
    for row in &columns.rows {
        let (Some(table), Some(column), Some(data_type)) = (
            row.first().and_then(Value::as_str),
            row.get(1).and_then(Value::as_str),
            row.get(2).and_then(Value::as_str),
        ) else {
            continue;
        };
        let column_entry = json!({ "name": column, "type": data_type });
        match tables
            .iter_mut()
            .find(|entry| entry["name"].as_str() == Some(table))
        {
            Some(entry) => {
                entry["columns"]
                    .as_array_mut()
                    .expect("columns array")
                    .push(column_entry);
            }
            None => tables.push(json!({ "name": table, "columns": [column_entry] })),
        }
    }
    Ok(json!({ "db_id": db_id.as_str(), "tables": tables }))
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
            task_type: request.args.operation_name().to_owned(),
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
    fn operation_name(&self) -> &'static str {
        match self {
            Self::Query(_) => "query",
            Self::Execute(_) => "execute",
            Self::Ingest(_) => "ingest",
            Self::Export(_) => "export",
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
                        json!({ "db": output.db.as_str(), "artifact": output.artifact.artifact_id }),
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
        .map(|artifact| json!({ "artifact": artifact.artifact_id }))
        .unwrap_or_else(|| json!({ "inline_rows": output.rows.len() }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_stateful_mode(false)
            .with_json_response(true)
            .with_cancellation_token(ct.child_token()),
    );
    let task_extension = Arc::new(TaskExtensionAdapter::new(
        Arc::new(DuckdbTaskExtension::new(state.clone())),
        ServerDiscovery::new(
            std::collections::BTreeMap::from([
                ("tools".to_owned(), json!({})),
                ("resources".to_owned(), json!({})),
            ]),
            TaskExtensionImplementation {
                name: "duckdb".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some(
                "Sandboxed arbitrary DuckDB SQL with durable final-extension tasks and shared artifacts."
                    .to_owned(),
            ),
        ),
    ));
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            task_extension,
            task_extension_middleware::<DuckdbTaskExtension>,
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
mod task_tests {
    use super::*;

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
