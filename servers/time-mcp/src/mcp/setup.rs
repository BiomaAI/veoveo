use std::sync::LazyLock;

use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{McpResource, McpServerContract, McpServerSetup, McpSetupError},
};
use veoveo_types::ResourceScheme;

use super::SERVER_DOCS;
use crate::{
    contract::{TimeDocument, TimeResource, TimeScope},
    uris,
};

pub(crate) struct TimeContract;
pub(crate) static SERVER_SETUP: LazyLock<McpServerSetup<TimeContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("Time MCP contract setup"));

impl McpServerContract for TimeContract {
    type Scope = TimeScope;
    type Resource = TimeResource;

    fn slug() -> ServerSlug {
        ServerSlug::new("time").expect("declared Time slug")
    }
    fn scheme() -> ResourceScheme {
        ResourceScheme::new("time").expect("declared Time scheme")
    }
    fn scopes() -> &'static [TimeScope] {
        TimeScope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_info()
    }
    fn resources() -> Result<Vec<McpResource<TimeResource>>, McpSetupError> {
        resources()
    }
    fn resource_templates() -> Vec<ResourceTemplate> {
        resource_templates()
    }
}

fn json_descriptor(
    address: TimeResource,
    title: &str,
    description: &str,
) -> Result<McpResource<TimeResource>, McpSetupError> {
    McpResource::new(address, |uri| {
        Resource::new(uri, title)
            .with_title(title)
            .with_description(description)
            .with_mime_type("application/json")
    })
}

fn resources() -> Result<Vec<McpResource<TimeResource>>, McpSetupError> {
    let mut resources = vec![json_descriptor(
        TimeResource::Docs,
        "Server documents",
        "Index of the crate documents embedded at build time.",
    )?];
    for doc in SERVER_DOCS.iter() {
        resources.push(McpResource::new(
            TimeResource::Document(TimeDocument::parse(doc.id).expect("declared Time document")),
            |uri| {
                Resource::new(uri, doc.title)
                    .with_title(doc.title)
                    .with_description("Crate document embedded at build time.")
                    .with_mime_type("text/markdown")
            },
        )?);
    }
    resources.push(json_descriptor(
        TimeResource::Contract,
        "Contract declaration",
        "Machine-readable contract revision and compliance declaration.",
    )?);
    for (address, title) in [
        (TimeResource::ClockCurrent, "Current authoritative time"),
        (TimeResource::ClockQuality, "Clock quality"),
        (TimeResource::AuthoritiesCurrent, "Active time authorities"),
        (
            TimeResource::Calendars { cursor: None },
            "Operational calendars",
        ),
        (TimeResource::Epochs { cursor: None }, "Mission epochs"),
        (TimeResource::Events { cursor: None }, "Temporal events"),
    ] {
        resources.push(json_descriptor(
            address,
            title,
            "Authorized Time domain resource.",
        )?);
    }
    resources.push(McpResource::new(TimeResource::TimelineApp, |uri| {
        veoveo_mcp_apps_extension::app_resource(uri, "timeline")
            .with_title("Timeline")
            .with_description(
                "Authoritative time, operational calendars, epochs, and temporal events.",
            )
    })?);
    Ok(resources)
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
    info.server_info = rmcp::model::Implementation::new("time", env!("CARGO_PKG_VERSION"));
    info.instructions = Some("Time interpretation and scheduling for agents. Resolve civil, military, GNSS, Unix, TAI, and mission-relative times against versioned TZDB and leap-second releases. Call `expand_schedule` and `validate_timeline` as MCP Tasks. When you pass a time to Map or Optimization, pass the resolved TimeInstant with its uncertainty, not a plain string.".to_owned());
    info
}

fn template(uri: &str, title: &str, description: &str) -> ResourceTemplate {
    ResourceTemplate::new(uri, title)
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}

fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        template(
            uris::CALENDARS_TEMPLATE,
            "Calendar page",
            "A page of 100 calendar versions.",
        ),
        template(
            uris::EPOCHS_TEMPLATE,
            "Epoch page",
            "A page of 100 mission epoch versions.",
        ),
        template(
            uris::EVENTS_TEMPLATE,
            "Event page",
            "A page of 100 owner-scoped events.",
        ),
        ResourceTemplate::new(uris::DOC_TEMPLATE, "Server document")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        template(
            uris::ZONE_TEMPLATE,
            "IANA time zone",
            "Zone interpretation under active TZDB.",
        ),
        template(
            uris::AUTHORITY_RELEASE_TEMPLATE,
            "Time authority release",
            "Immutable compiler-ready authority provenance.",
        ),
        template(
            uris::CALENDAR_TEMPLATE,
            "Operational calendar",
            "Versioned operational calendar.",
        ),
        template(
            uris::EPOCH_TEMPLATE,
            "Mission epoch",
            "Versioned mission epoch.",
        ),
        template(
            uris::EVENT_TEMPLATE,
            "Temporal event",
            "Owner-scoped temporal event.",
        ),
    ]
}
