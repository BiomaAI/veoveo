//! DuckDB MCP server.
//!
//! MCP surface:
//!   tool `query(db, sql, ...)` — read-only SQL, direct or task
//!   tool `execute(db, sql, ...)` — DDL/DML on an owned database, direct or task
//!   tool `ingest(db, table, source, mode)` — task-required source loading
//!   tool `export(db, selection, format)` — task-required artifact export
//!   resource `duckdb://dbs` — databases visible to the caller
//!   template `duckdb://db/{db_id}` — schema summary for one database
//!   template `duckdb://artifact/{sha256}` — immutable export artifact bytes
//!   template `duckdb://usage/task/{task_id}` — task usage rows

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
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_duckdb_mcp::{
    artifacts::ArtifactRepository,
    engine::{self, EngineSettings, FileExchange},
    state::{DuckdbState, TaskOwner},
    uris,
};
use veoveo_mcp_contract::{
    DuckDbExecuteOutput, DuckDbExecuteRequest, DuckDbExportOutput, DuckDbExportRequest,
    DuckDbIngestOutput, DuckDbIngestRequest, DuckDbQueryOutput, DuckDbQueryRequest,
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, InternalTokenSecret, Page,
    ServerSlug, TaskPayloadState, TaskStore, TelemetryGuard, TokenIssuer, UsageReport,
    init_server_telemetry, is_sha256, now_iso, paginate, public_allowed_hosts, related_task_meta,
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

use app_state::{AppState, Caps, ServerDirs, update_task};
use config::Args;
use host::validate_host;
use internal_auth::{
    InternalMcpAuthState, authenticate_internal_mcp, verify_internal_authorization,
};
use ownership::{
    database_readable, internal_caller, internal_identity, optional_task_owner, require_task_owner,
    resolve_readable_database, task_owner_allows, task_owner_from_identity,
};

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3000;
const SERVER_SLUG: &str = "duckdb";
const LIST_PAGE_SIZE: usize = 100;

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn direct_call_owner(identity: &veoveo_mcp_contract::GatewayInternalIdentity) -> TaskOwner {
    task_owner_from_identity(&format!("call-{}", uuid::Uuid::new_v4()), identity)
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
        description = "Run one read-only SQL statement against a database you own. Read-only is enforced by the connection, and SQL cannot touch files, the network, extensions, or engine settings. Inline output is capped; pass output = {mode: \"artifact\", format: \"parquet\"} for large results, which returns one duckdb://artifact/{sha256} link. To query another principal's data, have them export a snapshot to the artifact plane and grant it, then ingest it here with an artifact:// source.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<DuckDbQueryOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        execution(task_support = "optional")
    )]
    async fn query(
        &self,
        Parameters(args): Parameters<DuckDbQueryRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let caller = internal_caller(&context)?;
        let identity = internal_identity(&context)?;
        let owner = direct_call_owner(&identity);
        let output = sql_ops::query_op(&self.state, &caller, &identity, &owner, args).await?;
        outputs::query_result(&output, None)
    }

    #[tool(
        title = "Execute SQL on a DuckDB database",
        description = "Run DDL/DML SQL on a database owned by the caller, creating it when create_if_missing is set. Writes serialize per database. SQL cannot touch files, the network, extensions, or engine settings.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<DuckDbExecuteOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        ),
        execution(task_support = "optional")
    )]
    async fn execute(
        &self,
        Parameters(args): Parameters<DuckDbExecuteRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = internal_identity(&context)?;
        let output = sql_ops::execute_op(&self.state, &identity, args).await?;
        outputs::execute_result(&output, None)
    }

    #[tool(
        title = "Ingest data into a DuckDB table",
        description = "Load a typed source into one table: inline CSV, allowlisted HTTPS URIs, or an artifact:// reference to any hosted server's artifact (media output, timeseries RRD, optimization snapshot), resolved through the shared artifact plane under your identity and gated by its grant + label checks. The server fetches and resolves sources itself; SQL never reaches the network. Must be invoked as an MCP task; read tasks/result for the final row count.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<DuckDbIngestOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        ),
        execution(task_support = "required")
    )]
    async fn ingest(
        &self,
        Parameters(_args): Parameters<DuckDbIngestRequest>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "ingest requires task-based invocation",
            None,
        ))
    }

    #[tool(
        title = "Export DuckDB data to an artifact",
        description = "Export a table, a read-only SQL result, or a full owned-database snapshot to one immutable duckdb://artifact/{sha256} artifact (parquet, csv, or duck_db snapshot). Must be invoked as an MCP task; read tasks/result for the artifact link.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<DuckDbExportOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        ),
        execution(task_support = "required")
    )]
    async fn export(
        &self,
        Parameters(_args): Parameters<DuckDbExportRequest>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "export requires task-based invocation",
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

enum TaskArgs {
    Query(DuckDbQueryRequest),
    Execute(DuckDbExecuteRequest),
    Ingest(DuckDbIngestRequest),
    Export(DuckDbExportRequest),
}

fn parse_task_args(request: &CallToolRequestParams) -> Result<TaskArgs, McpError> {
    let arguments = Value::Object(request.arguments.clone().unwrap_or_default());
    let invalid = |err: serde_json::Error| {
        McpError::invalid_params(format!("invalid {} arguments: {err}", request.name), None)
    };
    match request.name.as_ref() {
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
        _ => Err(McpError::method_not_found::<
            rmcp::model::CallToolRequestMethod,
        >()),
    }
}

#[tool_handler]
impl ServerHandler for DuckdbMcp {
    fn get_info(&self) -> ServerInfo {
        let caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_tasks_with(TasksCapability::server_default())
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

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        let caller = internal_caller(&context)?;
        let identity = internal_identity(&context)?;
        let args = parse_task_args(&request)?;
        let progress_token = context.meta.get_progress_token();
        let ttl = request.task.as_ref().and_then(|task| task.ttl);
        let task_id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        let mut task = Task::new(task_id.clone(), TaskStatus::Working, now.clone(), now)
            .with_status_message("accepted")
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
            caller,
            identity,
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
        match self.state.tasks.await_payload_state(&request.task_id).await {
            TaskPayloadState::Completed(payload) => Ok(GetTaskPayloadResult::new(payload)),
            TaskPayloadState::Failed(error) => Err(McpError::internal_error(error, None)),
            TaskPayloadState::Cancelled => {
                Err(McpError::invalid_request("task was cancelled", None))
            }
            // await_payload_state blocks until terminal per MCP 2025-11-25;
            // Running here means the wait logic itself broke.
            TaskPayloadState::Running => Err(McpError::internal_error(
                "task payload wait ended while still running",
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
            Resource::new(uris::DBS_ROOT_URI, "dbs")
                .with_title("DuckDB databases")
                .with_description("Databases visible to the caller.")
                .with_mime_type("application/json"),
            Resource::new(uris::USAGE_ROOT_URI, "usage")
                .with_title("DuckDB usage ledger")
                .with_description("Index of task usage resources.")
                .with_mime_type("application/json"),
        ];
        for database in self
            .state
            .durable
            .databases()
            .map_err(|err| McpError::internal_error(err.to_string(), None))?
        {
            if !database_readable(&database, &identity) {
                continue;
            }
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
                .with_description("Server-owned immutable export artifact, addressed by sha256."),
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
            for database in self
                .state
                .durable
                .databases()
                .map_err(|err| McpError::internal_error(err.to_string(), None))?
            {
                if !database_readable(&database, &identity) {
                    continue;
                }
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
        if let Some(db_id) = uris::parse_db_uri(uri) {
            let schema = database_schema_document(&self.state, &identity, db_id).await?;
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(serde_json::to_string(&schema).unwrap_or_default(), uri)
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
            // The plane enforces access with the caller's identity; a denial
            // surfaces as an error rather than None.
            let caller = internal_caller(&context)?;
            let artifact = self
                .state
                .artifacts
                .get(&caller, sha256)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown artifact '{sha256}'"), None)
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
    let db_id = veoveo_mcp_contract::DuckDbDatabaseId::new(db_id)
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

#[allow(clippy::too_many_arguments)]
async fn run_task(
    state: Arc<AppState>,
    peer: rmcp::service::Peer<RoleServer>,
    task_id: String,
    caller: veoveo_mcp_contract::PlaneCaller,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    owner: TaskOwner,
    args: TaskArgs,
    progress_token: Option<rmcp::model::ProgressToken>,
) {
    macro_rules! fail {
        ($msg:expr) => {{
            let msg: String = $msg;
            tracing::warn!(task_id, "duckdb task failed: {msg}");
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
    veoveo_mcp_contract::notify_progress(&peer, &progress_token, 0.1, "running").await;
    let result = match args {
        TaskArgs::Query(request) => {
            match sql_ops::query_op(&state, &caller, &identity, &owner, request).await {
                Ok(output) => {
                    outputs::record_op_usage(
                        &state,
                        &task_id,
                        "query",
                        output.row_count,
                        json!({ "db": output_db_meta(&output) }),
                    );
                    outputs::query_result(&output, Some(&task_id))
                }
                Err(err) => fail!(format!("query failed: {}", err.message)),
            }
        }
        TaskArgs::Execute(request) => match sql_ops::execute_op(&state, &identity, request).await {
            Ok(output) => {
                outputs::record_op_usage(
                    &state,
                    &task_id,
                    "execute",
                    output.rows_changed,
                    json!({ "db": output.db.as_str(), "statements": output.statements }),
                );
                outputs::execute_result(&output, Some(&task_id))
            }
            Err(err) => fail!(format!("execute failed: {}", err.message)),
        },
        TaskArgs::Ingest(request) => {
            match sql_ops::ingest_op(&state, &caller, &identity, request).await {
                Ok(output) => {
                    outputs::record_op_usage(
                        &state,
                        &task_id,
                        "ingest",
                        output.rows_ingested,
                        json!({ "db": output.db.as_str(), "table": output.table }),
                    );
                    outputs::ingest_result(&output, Some(&task_id))
                }
                Err(err) => fail!(format!("ingest failed: {}", err.message)),
            }
        }
        TaskArgs::Export(request) => {
            match sql_ops::export_op(&state, &caller, &identity, &owner, request).await {
                Ok(output) => {
                    outputs::record_op_usage(
                        &state,
                        &task_id,
                        "export",
                        output.rows_exported,
                        json!({ "db": output.db.as_str(), "artifact": output.artifact.sha256 }),
                    );
                    outputs::export_result(&output, Some(&task_id))
                }
                Err(err) => fail!(format!("export failed: {}", err.message)),
            }
        }
    };
    let result = match result {
        Ok(result) => result,
        Err(err) => fail!(format!("result assembly failed: {}", err.message)),
    };
    veoveo_mcp_contract::notify_progress(&peer, &progress_token, 1.0, "completed").await;
    update_task(
        &state,
        &peer,
        &task_id,
        TaskStatus::Completed,
        "completed",
        serde_json::to_value(&result).ok(),
        None,
    )
    .await;
}

fn output_db_meta(output: &DuckDbQueryOutput) -> Value {
    output
        .artifact
        .as_ref()
        .map(|artifact| json!({ "artifact": artifact.sha256 }))
        .unwrap_or_else(|| json!({ "inline_rows": output.rows.len() }))
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
                "rejected duckdb artifact download: {message}"
            );
            return (StatusCode::UNAUTHORIZED, "gateway authorization required").into_response();
        }
    };
    let Some(bearer) = internal_auth::bearer_from_headers(&headers) else {
        return (StatusCode::UNAUTHORIZED, "gateway authorization required").into_response();
    };
    let caller = ownership::caller_from(identity, bearer);
    // The plane enforces access; a denial reads as None (not found) here.
    let artifact = match state.artifacts.get(&caller, &sha256).await {
        Ok(Some(artifact)) => artifact,
        Ok(None) => return (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(err) => {
            tracing::warn!(artifact_sha256 = sha256, "artifact download failed: {err}");
            return (StatusCode::NOT_FOUND, "not found").into_response();
        }
    };

    let mut headers = HeaderMap::new();
    if let Some(mime) = artifact.metadata.mime_type.as_deref()
        && let Ok(value) = HeaderValue::from_str(mime)
    {
        headers.insert(CONTENT_TYPE, value);
    }
    let filename = artifact
        .metadata
        .filename
        .as_deref()
        .unwrap_or("export.bin")
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
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-duckdb-mcp", "info,veoveo_duckdb_mcp=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    for dir in [&args.database_dir, &args.exchange_dir, &args.spill_dir] {
        std::fs::create_dir_all(dir)?;
    }
    let durable = DuckdbState::open(&args.state_db)?;
    let internal_token_verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        InternalTokenSecret::new(args.internal_token_secret.clone())?,
    );
    let artifacts = ArtifactRepository::new(args.artifact_service_url.clone());
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
    let engine_settings = EngineSettings {
        memory_limit: args.engine_memory_limit.clone(),
        threads: args.engine_threads,
        spill_dir: args.spill_dir.clone(),
    };
    let http = reqwest::Client::builder()
        .user_agent("veoveo-duckdb-mcp")
        .build()?;
    let state = Arc::new(AppState::new(
        tasks,
        durable,
        artifacts,
        internal_token_verifier.clone(),
        task_owners,
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
        args.allow_ingest_hosts.clone(),
        http,
    ));

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
