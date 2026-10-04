//! UAV-owned MCP declaration, checked before service construction and discovery.
use crate::{
    contract::{UavDocument, UavResource, UavScope},
    uris,
};
use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use std::sync::LazyLock;
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceScheme, ResourceTemplateUri};

pub(super) const SERVER_SLUG: &str = "uav-sim";
pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!(SERVER_SLUG));
pub(super) struct UavContract;
pub(super) static SERVER_SETUP: LazyLock<McpServerSetup<UavContract>> = LazyLock::new(|| {
    McpServerSetup::new().expect("UAV MCP declaration must satisfy the server contract")
});

impl McpServerContract for UavContract {
    type Scope = UavScope;
    type Resource = UavResource;
    fn slug() -> ServerSlug {
        ServerSlug::parse(SERVER_SLUG).expect("declared UAV slug")
    }
    fn scheme() -> ResourceScheme {
        ResourceScheme::parse(uris::SCHEME).expect("declared UAV scheme")
    }
    fn scopes() -> &'static [UavScope] {
        UavScope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_config()
    }
    fn resources() -> Result<Vec<McpResource<UavResource>>, McpSetupError> {
        resources()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        resource_templates()
    }
}

fn json_descriptor(
    address: UavResource,
    title: &str,
    description: &str,
) -> Result<McpResource<UavResource>, McpSetupError> {
    McpResource::new(address, |uri| {
        Resource::new(uri, title)
            .with_title(title)
            .with_description(description)
            .with_mime_type("application/json")
    })
}

fn resources() -> Result<Vec<McpResource<UavResource>>, McpSetupError> {
    let mut resources = vec![json_descriptor(
        UavResource::Docs,
        "Server documents",
        "Index of the crate documents embedded at build time.",
    )?];
    for doc in SERVER_DOCS.iter() {
        resources.push(McpResource::new(
            UavResource::Document(
                UavDocument::parse(doc.id).map_err(|_| McpSetupError::InvalidDocument)?,
            ),
            |uri| {
                Resource::new(uri, doc.title)
                    .with_title(doc.title)
                    .with_description("Crate document embedded at build time.")
                    .with_mime_type("text/markdown")
            },
        )?);
    }
    for (address, title, description) in [
        (
            UavResource::Contract,
            "Contract declaration",
            "Machine-readable contract revision, compliance, and capability inventory.",
        ),
        (
            UavResource::Sessions,
            "Simulation sessions",
            "Authorized simulation session index.",
        ),
        (
            UavResource::Missions { cursor: None },
            "Simulation missions",
            "Paged authorized mission resource URIs.",
        ),
        (
            UavResource::Usage { cursor: None },
            "Simulation task usage",
            "Paged authorized task usage resource URIs.",
        ),
        (
            UavResource::ControlGrants { cursor: None },
            "Vehicle control grants",
            "Paged UAV principal-to-vehicle grants.",
        ),
        (
            UavResource::MissionPlans { cursor: None },
            "Vehicle mission plans",
            "Paged plans admitted from Map route handoffs.",
        ),
    ] {
        resources.push(json_descriptor(address, title, description)?);
    }
    resources.push(McpResource::new(UavResource::LiveApp, |uri| {
        app_descriptor(uri, veoveo_mcp_apps_extension::ResourceUiMeta::default())
    })?);
    Ok(resources)
}

fn server_config() -> ServerConfig {
    let mut capabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_prompts()
        .enable_resources()
        .enable_resources_subscribe()
        .enable_resources_list_changed()
        .enable_completions()
        .build();
    veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
    capabilities.extensions.get_or_insert_default().insert(
        rmcp::model::TASKS_EXTENSION_ID.to_owned(),
        rmcp::model::JsonObject::new(),
    );
    let mut info = ServerConfig::default();
    info.capabilities = capabilities;
    info.server_info = rmcp::model::Implementation::new(SERVER_SLUG, env!("CARGO_PKG_VERSION"));
    info.instructions = Some(
            "Fly simulated UAVs. To fly a mission: (1) call `list_active_vehicle_control_grants` to find your vehicle and its Map mobility profile; (2) get a route handoff from Map MCP; (3) call `prepare_vehicle_mission` with that handoff; (4) call `execute_vehicle_mission_plan` as an MCP Task. Scenarios and sensor captures also run as MCP Tasks, and an interrupted flight is never retried automatically. Watch the operator cameras in the ui://uav-sim/live.html app."
                .to_owned(),
        );
    info
}

fn json_template(
    uri: &str,
    title: &str,
    description: &str,
) -> Result<McpResourceTemplate, McpSetupError> {
    template(uri, title, description, "application/json")
}
fn template(
    uri: &str,
    title: &str,
    description: &str,
    mime: &str,
) -> Result<McpResourceTemplate, McpSetupError> {
    let uri = ResourceTemplateUri::new(uri).map_err(|_| McpSetupError::InvalidTemplate)?;
    McpResourceTemplate::new(uri, |uri| {
        ResourceTemplate::new(uri, title)
            .with_title(title)
            .with_description(description)
            .with_mime_type(mime)
    })
}

fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
    vec![
        json_template(
            uris::CONTROL_GRANTS_PAGE_TEMPLATE,
            "Vehicle control grant page",
            "100 visible grants per page.",
        ),
        json_template(
            uris::MISSION_PLANS_PAGE_TEMPLATE,
            "Vehicle mission plan page",
            "100 visible plans per page.",
        ),
        json_template(
            uris::MISSIONS_PAGE_TEMPLATE,
            "Mission page",
            "100 authorized mission resource URIs per page.",
        ),
        json_template(
            uris::USAGE_PAGE_TEMPLATE,
            "Task usage page",
            "100 authorized task usage URIs per page.",
        ),
        template(
            uris::DOC_TEMPLATE,
            "Server document",
            "Embedded crate document body (contract C18).",
            "text/markdown",
        ),
        json_template(
            uris::SESSION_TEMPLATE,
            "Simulation session",
            "Typed session state.",
        ),
        json_template(
            uris::WORLD_TEMPLATE,
            "Simulation world",
            "Frame, georeference, and world clock state.",
        ),
        json_template(
            uris::TILES_TEMPLATE,
            "Simulation tiles",
            "Google Photorealistic 3D Tiles load state inside the simulator.",
        ),
        json_template(
            uris::VEHICLES_TEMPLATE,
            "Simulation vehicles",
            "Vehicle inventory for one session.",
        ),
        json_template(
            uris::VEHICLE_TEMPLATE,
            "Simulation vehicle",
            "Typed state for one simulated vehicle.",
        ),
        json_template(
            uris::RECORDINGS_TEMPLATE,
            "Simulation recordings",
            "Governed recording identities emitted by one session.",
        ),
        json_template(
            uris::LIVE_CAMERAS_TEMPLATE,
            "Live cameras",
            "Authoritative operator-camera inventory.",
        ),
        json_template(
            uris::LIVE_CAMERA_TEMPLATE,
            "Live camera",
            "One authoritative operator camera.",
        ),
        json_template(
            uris::STREAM_PRODUCTS_TEMPLATE,
            "Stream products",
            "Stable camera-owned rendered and encoded products.",
        ),
        json_template(
            uris::STREAM_PRODUCT_TEMPLATE,
            "Stream product",
            "One camera-owned RTX render and NVIDIA NVENC product shared across viewers.",
        ),
        json_template(
            uris::LIVE_VIEWS_TEMPLATE,
            "Live views",
            "Caller-visible live-view authorizations without secret tokens.",
        ),
        json_template(
            uris::LIVE_VIEWS_PAGE_TEMPLATE,
            "Live view page",
            "100 caller-visible live-view authorizations per page.",
        ),
        json_template(
            uris::LIVE_VIEW_TEMPLATE,
            "Live view",
            "One caller-visible live-view authorization without its token.",
        ),
        json_template(
            uris::MISSION_TEMPLATE,
            "Simulation mission",
            "Authorized durable mission task state.",
        ),
        json_template(
            uris::CONTROL_GRANT_TEMPLATE,
            "Vehicle control grant",
            "One UAV-owned principal-to-vehicle authority grant.",
        ),
        json_template(
            uris::MISSION_PLAN_TEMPLATE,
            "Vehicle mission plan",
            "One UAV-owned plan admitted from a Map route handoff.",
        ),
        json_template(
            uris::USAGE_TASK_TEMPLATE,
            "Simulation task usage",
            "Usage report for one authorized task.",
        ),
    ]
    .into_iter()
    .collect()
}

const LIVE_APP_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNCIgaGVpZ2h0PSIyNCIgdmlld0JveD0iMCAwIDI0IDI0IiBmaWxsPSJub25lIiBzdHJva2U9IiM2NmU0ZmYiIHN0cm9rZS13aWR0aD0iMiI+PHJlY3QgeD0iMiIgeT0iNSIgd2lkdGg9IjIwIiBoZWlnaHQ9IjE0IiByeD0iMiIvPjxwYXRoIGQ9Im04IDlsNiAzLTYgM3oiLz48L3N2Zz4=";

fn app_descriptor(uri: &str, metadata: veoveo_mcp_apps_extension::ResourceUiMeta) -> Resource {
    veoveo_mcp_apps_extension::app_resource_with_meta(uri, "uav-sim-live-app", metadata)
        .with_title("Live Cameras")
        .with_description("Authoritative simulator cameras tiled into one native NVIDIA NVENC product shared across viewers.")
        .with_icons(vec![rmcp::model::Icon::new(LIVE_APP_ICON)])
}

/// Installation and caller metadata may vary; the typed App address cannot change.
pub(super) fn live_app_resource(
    connect_origin: &str,
    agent_message_targets: &[String],
) -> Result<Resource, McpSetupError> {
    let resource = McpResource::new(UavResource::LiveApp, |uri| {
        let resource = app_descriptor(
            uri,
            veoveo_mcp_apps_extension::ResourceUiMeta {
                csp: Some(veoveo_mcp_apps_extension::UiCsp {
                    connect_domains: vec![connect_origin.to_owned()],
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        if agent_message_targets.is_empty() {
            resource
        } else {
            veoveo_mcp_apps_extension::with_agent_message_targets(
                resource,
                agent_message_targets.iter().cloned(),
            )
            .expect("validated UAV App agent message targets")
        }
    })?;
    Ok(resource.descriptor().clone())
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
