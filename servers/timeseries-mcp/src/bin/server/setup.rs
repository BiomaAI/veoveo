//! Startup configuration and discovery declarations for the hosted server.
use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use veoveo_timeseries_mcp::{
    contract::{TimeseriesTaskUsageUri, TimeseriesUsageIndexUri},
    forecast::RRD_MIME_TYPE,
    uris,
};
fn resources() -> Vec<Resource> {
    let mut resources = vec![
        veoveo_mcp_apps_extension::app_resource(uris::FORECAST_APP_URI, "forecast-app")
            .with_title("Forecasts")
            .with_description(
                "Interactive MCP App rendering forecast previews and re-running the \
                     forecast tool.",
            ),
        Resource::new(TimeseriesUsageIndexUri::ROOT, "usage")
            .with_title("Timeseries usage ledger")
            .with_description("Index of task usage resources.")
            .with_mime_type("application/json"),
        Resource::new(uris::DOCS_URI, "Server documents")
            .with_title("Server documents")
            .with_description("Index of the crate documents embedded at build time.")
            .with_mime_type("application/json"),
        Resource::new(uris::CONTRACT_URI, "Contract declaration")
            .with_title("Contract declaration")
            .with_description(
                "Machine-readable contract revision, compliance, and capability inventory.",
            )
            .with_mime_type("application/json"),
    ];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(
                TimeseriesResource::Document(
                    TimeseriesDocument::parse(doc.id).expect("embedded document"),
                )
                .to_uri()
                .expect("typed document URI")
                .to_string(),
                doc.title,
            )
            .with_title(doc.title)
            .with_description("Crate document embedded at build time.")
            .with_mime_type("text/markdown"),
        );
    }
    // Growing task usage stays behind the bounded usage index and exact
    // resource template instead of inflating the MCP resource catalog.
    resources.sort_by(|left, right| left.uri.cmp(&right.uri));

    resources
}
fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
            .with_title("Timeseries artifact")
            .with_description(
                "Server-owned immutable Rerun RRD artifact, addressed by occurrence id.",
            )
            .with_mime_type(RRD_MIME_TYPE),
        ResourceTemplate::new(TimeseriesTaskUsageUri::TEMPLATE, "usage")
            .with_title("Timeseries task usage")
            .with_description("Usage rows for one task, addressed by task id.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(TimeseriesUsageIndexUri::TEMPLATE, "usage-page")
            .with_title("Timeseries usage page")
            .with_description("Bounded usage index page selected by its opaque cursor.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::DOC_TEMPLATE, "Server document")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
    ]
}

use std::sync::LazyLock;
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_timeseries_mcp::contract::{TimeseriesDocument, TimeseriesResource, TimeseriesScope};
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceTemplateUri};

pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("timeseries"));
pub(super) struct TimeseriesContract;
pub(super) static SERVER_SETUP: LazyLock<McpServerSetup<TimeseriesContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("declared Timeseries MCP setup"));
impl McpServerContract for TimeseriesContract {
    type Scope = TimeseriesScope;
    type Resource = TimeseriesResource;
    fn slug() -> ServerSlug {
        ServerSlug::new("timeseries").expect("declared slug")
    }
    fn scheme() -> ResourceScheme {
        uris::SCHEME.clone()
    }
    fn scopes() -> &'static [Self::Scope] {
        &[]
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_info()
    }
    fn resources() -> Result<Vec<McpResource<Self::Resource>>, McpSetupError> {
        resources()
            .into_iter()
            .map(|descriptor| {
                let address = TimeseriesResource::parse(&descriptor.uri)
                    .map_err(|_| McpSetupError::InvalidResource)?;
                McpResource::new(address, |_| descriptor)
            })
            .collect()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        resource_templates()
            .into_iter()
            .map(|descriptor| {
                let template = ResourceTemplateUri::new(&descriptor.uri_template)
                    .map_err(|_| McpSetupError::InvalidTemplate)?;
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
    info.server_info = rmcp::model::Implementation::new("timeseries", env!("CARGO_PKG_VERSION"));
    info.instructions = Some(
        "Timeseries forecasting server. Call `forecast` with a typed DuckDB source and table \
         mapping; the result contains a timeseries://artifact/{artifact_id} Rerun RRD output."
            .into(),
    );
    info
}

#[cfg(test)]
mod setup_tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn checked_setup_qualifies_templates_capabilities_and_app_metadata() {
        let setup = &*SERVER_SETUP;
        assert_eq!(setup.resources().len(), 6);
        assert_eq!(setup.resource_templates().len(), 4);
        assert!(setup.scope_names().is_empty());
        assert!(
            TimeseriesScope::try_from(&veoveo_types::ScopeName::new("external:read").unwrap())
                .is_err()
        );
        let capabilities = &setup.server_config().capabilities;
        assert!(
            !capabilities
                .resources
                .as_ref()
                .unwrap()
                .subscribe
                .unwrap_or(false)
        );
        assert!(
            !capabilities
                .resources
                .as_ref()
                .unwrap()
                .list_changed
                .unwrap_or(false)
        );
        assert!(
            capabilities
                .extensions
                .as_ref()
                .unwrap()
                .contains_key(rmcp::model::TASKS_EXTENSION_ID)
        );
        let app = setup
            .resources()
            .iter()
            .find(|resource| resource.descriptor().uri == uris::FORECAST_APP_URI)
            .unwrap();
        let original = resources()
            .into_iter()
            .find(|resource| resource.uri == uris::FORECAST_APP_URI)
            .unwrap();
        assert_eq!(
            serde_json::to_value(app.descriptor()).unwrap(),
            serde_json::to_value(original).unwrap()
        );
        let task = veoveo_types::TaskId::new();
        let artifact = veoveo_artifact_contract::ArtifactId::new();
        let variables = BTreeMap::from([
            ("doc_id".to_owned(), "agents".to_owned()),
            ("task_id".to_owned(), task.to_string()),
            ("artifact_id".to_owned(), artifact.to_string()),
            ("world_id".to_owned(), "survey".to_owned()),
            ("revision_id".to_owned(), "revision-1".to_owned()),
            ("frame_id".to_owned(), "camera".to_owned()),
            ("operation_id".to_owned(), "conversion-1".to_owned()),
        ]);
        for template in setup.resource_templates() {
            let uri = template.template().expand_scalars(&variables).unwrap();
            let address = TimeseriesResource::parse(uri.as_str()).unwrap();
            assert_eq!(address.to_uri().unwrap(), uri);
            if template.template().variables().any(|name| name == "cursor") {
                let mut paged = variables.clone();
                let cursor = veoveo_timeseries_mcp::contract::TimeseriesUsageCursor::new(task)
                    .unwrap()
                    .as_str()
                    .to_owned();
                paged.insert("cursor".into(), cursor);
                let uri = template.template().expand_scalars(&paged).unwrap();
                assert_eq!(
                    TimeseriesResource::parse(uri.as_str())
                        .unwrap()
                        .to_uri()
                        .unwrap(),
                    uri
                );
            }
        }
    }
}
