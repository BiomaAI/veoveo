//! Startup and discovery share one checked, server-owned MCP declaration.
use super::{
    SERVER_DOCS,
    discovery::{declared_resources, declared_templates},
};
use crate::contract::{MapAddress, MapScope};
use rmcp::model::{ServerCapabilities, ServerConfig};
use std::sync::LazyLock;
use veoveo_mcp_contract::{
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceScheme, ResourceTemplateUri, ServerSlug};

pub struct MapContract;
pub(crate) static SERVER_SETUP: LazyLock<McpServerSetup<MapContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("declared Map MCP setup"));
impl McpServerContract for MapContract {
    type Scope = MapScope;
    type Resource = MapAddress;
    fn slug() -> ServerSlug {
        ServerSlug::new("map").expect("declared Map slug")
    }
    fn scheme() -> ResourceScheme {
        crate::uris::SCHEME.clone()
    }
    fn scopes() -> &'static [MapScope] {
        MapScope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_info()
    }
    fn resources() -> Result<Vec<McpResource<MapAddress>>, McpSetupError> {
        declared_resources()
            .into_iter()
            .map(|descriptor| {
                let address = MapAddress::parse(&descriptor.uri)
                    .map_err(|_| McpSetupError::InvalidResource)?;
                McpResource::new(address, |_| descriptor)
            })
            .collect()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        declared_templates()
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
    let mut capabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_prompts()
        .enable_resources()
        .enable_resources_subscribe()
        .enable_completions()
        .build();
    veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
    capabilities.extensions.get_or_insert_default().insert(
        rmcp::model::TASKS_EXTENSION_ID.to_owned(),
        rmcp::model::JsonObject::new(),
    );
    let mut info = ServerConfig::default();
    info.capabilities = capabilities;
    info.server_info = rmcp::model::Implementation::new("map", env!("CARGO_PKG_VERSION"));
    info.instructions = Some(
            "Geography, your own feature layers, and route planning for people, road and off-road vehicles, rail, maritime, and aviation. For routes, call `route` or `route_matrix` as MCP Tasks with an explicit mobility profile and departure time. To feed Optimization MCP, call `build_travel_model`. A route with `planning_advisory` status is guidance, not a certified plan. For your own data, create GeoJSON/JSON-FG feature layers in your Work Context, edit them with changesets, query them with CQL2 JSON, and publish fixed versions. Import, export, GeoPackage inspection, and vector-tile builds run as MCP Tasks. Your features never affect routing. The ui://map/workspace.html app shows compositions and layers interactively. Managing sources, acquisitions, releases, and mobility profiles requires the map:admin scope."
                .to_owned(),
        );
    info
}
