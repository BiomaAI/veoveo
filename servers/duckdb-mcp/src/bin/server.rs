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

use std::{
    net::SocketAddr,
    sync::{Arc, LazyLock},
};

use clap::Parser;
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ReadResourceRequestParams, Tool},
    service::RequestContext,
    tool_router,
};
use veoveo_duckdb_mcp::{
    artifacts::ArtifactRepository,
    contract::{
        DuckDbExecuteOutput, DuckDbExecuteRequest, DuckDbExportOutput, DuckDbExportRequest,
        DuckDbIngestOutput, DuckDbIngestRequest, DuckDbQueryOutput, DuckDbQueryRequest,
    },
    engine::{self, EngineSettings, TrustedExtension},
    uris,
};
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, TelemetryGuard,
    hosting::{
        DomainAddress, DomainRead, DomainServer, Hosted, HostedServer, gateway_identity,
        plane_caller,
    },
    init_server_telemetry,
    server_contract::McpServerSetup,
};
use veoveo_task_runtime::{DurableTasks, TaskError, TaskRuntime, TaskRuntimeConfig};

#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/artifact_output.rs"]
mod artifact_output;
#[path = "server/config.rs"]
mod config;
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
#[path = "server/tasks.rs"]
mod tasks;
#[cfg(test)]
#[path = "server/test_support.rs"]
mod test_support;

use app_state::{AppState, Caps, ServerDirs};
use artifact_output::ArtifactWriter;
use config::Args;
use setup::DuckDbContract;
use task_extension::DuckdbTaskService;
use tasks::resume_duckdb_task;

const SERVER_SLUG: &str = "duckdb";

#[cfg(test)]
use setup::SERVER_DOCS;

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// DuckDB's domain: tools and admitted resource reads. The host supplies the
/// rest of the MCP surface.
#[derive(Clone)]
struct DuckdbMcp {
    state: Arc<AppState>,
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
        let caller = plane_caller(&context)?;
        let identity = gateway_identity(&context)?;
        let writer = ArtifactWriter::caller(caller);
        let output = sql_ops::query_op(&self.state, &writer, &identity, args).await?;
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
        let identity = gateway_identity(&context)?;
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

impl DomainServer for DuckdbMcp {
    type Contract = DuckDbContract;

    fn setup() -> &'static McpServerSetup<DuckDbContract> {
        &setup::SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    fn describe_tool(&self, tool: Tool) -> Tool {
        veoveo_mcp_apps_extension::link_tool_to_app(
            tool,
            uris::WORKBENCH_APP_URI,
            &[
                veoveo_mcp_apps_extension::UiVisibility::Model,
                veoveo_mcp_apps_extension::UiVisibility::App,
            ],
        )
    }

    async fn read(
        &self,
        address: DomainAddress<DuckDbContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        resources::read(&self.state, address, &request.uri, context)
            .await
            .map(DomainRead::private)
    }
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

    let readiness_store = state.tasks.platform_store().clone();
    let server = HostedServer::for_domain::<DuckdbMcp>()
        .deployment(&public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .readiness(move || {
            let store = readiness_store.clone();
            async move {
                matches!(
                    tokio::time::timeout(std::time::Duration::from_secs(5), store.healthcheck())
                        .await,
                    Ok(Ok(()))
                )
            }
        })
        .internal_trust(GatewayInternalTrustBundle::from_json(
            &args.internal_trust_jwks,
        )?)?
        .handler(move || {
            Hosted::new(DuckdbMcp::new(state.clone())).with_tasks(DurableTasks::tasks_only(
                DuckdbTaskService::new(state.clone()),
            ))
        })
        .build();
    server
        .serve(SocketAddr::from(([0, 0, 0, 0], args.port)))
        .await
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
        assert_eq!(declaration.server().as_str(), "duckdb");
        assert_eq!(declaration.contract_revision(), CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance()
                .iter()
                .find(|item| item.id.as_str() == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        for item in declaration.compliance() {
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
mod tool_tests {
    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!super::DuckdbMcp::tool_router().list_all().is_empty());
    }
}
#[cfg(test)]
#[path = "server/tool_input_tests.rs"]
mod tool_input_tests;

#[cfg(test)]
#[path = "../../../../testing/fixtures/store.rs"]
mod store_fixture;
