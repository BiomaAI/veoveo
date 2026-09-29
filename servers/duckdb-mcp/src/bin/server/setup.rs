//! Hosted declarations checked before Store initialization and recovery.
use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use std::sync::LazyLock;
use veoveo_duckdb_mcp::{
    contract::{
        DuckDbDocument, DuckDbResource, DuckDbScope, DuckDbTaskUsageUri, DuckDbUsageIndexUri,
    },
    uris,
};
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceScheme, ResourceTemplateUri};

pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("duckdb"));
pub(super) struct DuckDbContract;
pub(super) static SERVER_SETUP: LazyLock<McpServerSetup<DuckDbContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("DuckDB MCP setup"));
impl McpServerContract for DuckDbContract {
    type Scope = DuckDbScope;
    type Resource = DuckDbResource;
    fn slug() -> ServerSlug {
        ServerSlug::new("duckdb").expect("declared DuckDB slug")
    }
    fn scheme() -> ResourceScheme {
        uris::SCHEME.clone()
    }
    fn scopes() -> &'static [DuckDbScope] {
        &[]
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_info()
    }
    fn resources() -> Result<Vec<McpResource<DuckDbResource>>, McpSetupError> {
        resource_catalog()
            .into_iter()
            .map(|descriptor| {
                let resource = DuckDbResource::parse(&descriptor.uri)
                    .map_err(|_| McpSetupError::InvalidResource)?;
                McpResource::new(resource, |_| descriptor)
            })
            .collect()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        resource_templates()
            .into_iter()
            .map(|descriptor| {
                let template = ResourceTemplateUri::new(descriptor.uri_template.as_str())
                    .map_err(|_| McpSetupError::InvalidResource)?;
                McpResourceTemplate::new(template, |_| descriptor)
            })
            .collect()
    }
}
fn server_info() -> ServerConfig {
    let mut caps: ServerCapabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_resources()
        .build();
    veoveo_mcp_apps_extension::extend_capabilities(&mut caps);
    caps.extensions.get_or_insert_default().insert(
        rmcp::model::TASKS_EXTENSION_ID.to_owned(),
        rmcp::model::JsonObject::new(),
    );
    let mut info = ServerConfig::default();
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

fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![
        Resource::new(uris::DOCS_URI, "docs")
            .with_title("Server documents")
            .with_description("Index of the crate documents embedded at build time.")
            .with_mime_type("application/json"),
    ];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(
                uris::doc_uri(DuckDbDocument::parse(doc.id).expect("embedded DuckDB document")),
                doc.title,
            )
            .with_title(doc.title)
            .with_description("Crate document embedded at build time.")
            .with_mime_type("text/markdown"),
        );
    }
    resources.push(
        Resource::new(uris::CONTRACT_URI, "contract")
            .with_title("Contract declaration")
            .with_description(
                "Machine-readable contract revision, compliance, and capability inventory.",
            )
            .with_mime_type("application/json"),
    );
    resources
}

/// Discovery declarations do not enumerate database files or Task usage.
fn resource_catalog() -> Vec<Resource> {
    let mut resources = well_known_resources();
    resources.extend([
        veoveo_mcp_apps_extension::app_resource(uris::WORKBENCH_APP_URI, "workbench")
            .with_title("Workbench")
            .with_description("Owner-scoped analytical SQL, ingestion, and export."),
        Resource::new(uris::DBS_ROOT_URI, "dbs")
            .with_title("DuckDB databases")
            .with_description("Databases visible to the caller.")
            .with_mime_type("application/json"),
        Resource::new(DuckDbUsageIndexUri::ROOT, "usage")
            .with_title("DuckDB usage ledger")
            .with_description("Index of task usage resources.")
            .with_mime_type("application/json"),
    ]);
    resources
}

/// Templates served by `list_resource_templates` and declared in the
/// `duckdb://contract` capability inventory.
fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(uris::DBS_TEMPLATE, "database-page")
            .with_title("DuckDB databases")
            .with_description("Current owner database names, 100 per page.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::DOC_TEMPLATE, "doc")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        ResourceTemplate::new(uris::DB_TEMPLATE, "db")
            .with_title("DuckDB database schema")
            .with_description("Tables and columns for one visible database.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
            .with_title("DuckDB artifact")
            .with_description(
                "Server-owned immutable export artifact, addressed by occurrence id.",
            ),
        ResourceTemplate::new(DuckDbUsageIndexUri::TEMPLATE, "usage-page")
            .with_title("DuckDB usage ledger")
            .with_description("Caller-owned usage Tasks in pages of at most 100.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(DuckDbTaskUsageUri::TEMPLATE, "usage")
            .with_title("DuckDB task usage")
            .with_description("Usage rows for one task, addressed by task id.")
            .with_mime_type("application/json"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_discovery_needs_no_store_engine_or_domain_scope() {
        use veoveo_types::ResourceAddress;
        let setup = &*SERVER_SETUP;
        assert!(setup.scope_names().is_empty());
        for resource in setup.resources() {
            DuckDbResource::parse(resource.descriptor().uri.as_str()).unwrap();
        }
        assert_eq!(setup.resource_templates().len(), 6);
        assert!(
            setup
                .resources()
                .iter()
                .any(|r| r.descriptor().uri == uris::DBS_ROOT_URI)
        );
        let config = serde_json::to_value(setup.server_config()).unwrap();
        assert_ne!(config["capabilities"]["resources"]["subscribe"], true);
        assert_ne!(config["capabilities"]["resources"]["listChanged"], true);
        assert_eq!(
            DuckDbResource::Databases(None).to_uri().unwrap().as_str(),
            uris::DBS_ROOT_URI
        );
    }
}
