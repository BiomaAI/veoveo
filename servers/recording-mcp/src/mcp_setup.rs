//! Recording declarations checked before Store, cache or Redap initialization.
use crate::{
    admin::SERVER_DOCS,
    contract::{RecordingDocument, RecordingResource, RecordingScope},
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

pub struct RecordingContract;
pub static SERVER_SETUP: LazyLock<McpServerSetup<RecordingContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("Recording MCP contract setup"));

impl McpServerContract for RecordingContract {
    type Scope = RecordingScope;
    type Resource = RecordingResource;
    fn slug() -> ServerSlug {
        ServerSlug::parse("recording").expect("declared Recording slug")
    }
    fn scheme() -> ResourceScheme {
        ResourceScheme::parse("recording").expect("declared Recording scheme")
    }
    fn scopes() -> &'static [RecordingScope] {
        RecordingScope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_config()
    }
    fn resources() -> Result<Vec<McpResource<RecordingResource>>, McpSetupError> {
        descriptors()
            .into_iter()
            .map(|descriptor| {
                let address = RecordingResource::parse(&descriptor.uri)
                    .map_err(|_| McpSetupError::InvalidResource)?;
                McpResource::new(address, |_| descriptor)
            })
            .collect()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        templates()
            .into_iter()
            .map(|descriptor| {
                let template = ResourceTemplateUri::new(descriptor.uri_template.clone())
                    .map_err(|_| McpSetupError::InvalidTemplate)?;
                McpResourceTemplate::new(template, |_| descriptor)
            })
            .collect()
    }
}

fn server_config() -> ServerConfig {
    let mut capabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_prompts()
        .enable_resources()
        .enable_resources_subscribe()
        .enable_completions()
        .build();
    veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
    let mut info = ServerConfig::default();
    info.capabilities = capabilities;
    info.server_info = rmcp::model::Implementation::new("recording", env!("CARGO_PKG_VERSION"));
    info.instructions = Some(
            "The installation's recording catalog. Find recordings through resources. Use `create_recording_projection` to extract selected entities and components as Apache Arrow. With the recording:seal scope, you can seal a frozen recording; sealing returns artifact:// URIs, and artifact permissions control who can read or share them after that."
                .to_owned(),
        );
    info
}

fn descriptors() -> Vec<Resource> {
    let mut resources = vec![
        veoveo_mcp_apps_extension::app_resource(uris::EXPLORER_APP_URI, "explorer")
            .with_title("Explorer")
            .with_description(
                "Governed recording catalog, lifecycle, and bounded timeline queries.",
            ),
        Resource::new(uris::DOCS_URI, "recording docs")
            .with_title("Server documents")
            .with_description("Index of the crate documents embedded at build time.")
            .with_mime_type("application/json"),
        Resource::new(uris::CONTRACT_URI, "recording contract")
            .with_title("Contract declaration")
            .with_description(
                "Machine-readable contract revision, compliance, and capability inventory.",
            )
            .with_mime_type("application/json"),
        Resource::new(uris::CATALOG_URI, "recording catalog")
            .with_title("Recording catalog")
            .with_description("Authorized recording lifecycle and artifact index.")
            .with_mime_type("application/json"),
    ];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(
                uris::doc_uri(
                    RecordingDocument::parse(doc.id).expect("declared Recording document"),
                ),
                doc.title,
            )
            .with_title(doc.title)
            .with_description("Crate document embedded at build time.")
            .with_mime_type("text/markdown"),
        );
    }
    resources
}

fn templates() -> Vec<ResourceTemplate> {
    vec![
            ResourceTemplate::new(uris::CATALOG_TEMPLATE, "recording catalog page")
                .with_title("Recording catalog page")
                .with_description("Authorized recordings, 100 per page. Subscribe to recording://catalog for changes.")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::DOC_TEMPLATE, "doc")
                .with_title("Server document")
                .with_description("Embedded crate document body (contract C18).")
                .with_mime_type("text/markdown"),
            ResourceTemplate::new(uris::RECORDING_TEMPLATE, "recording")
                .with_title("Recording")
                .with_description("Governed recording metadata by UUIDv7.")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::LAYERS_TEMPLATE, "recording layers")
                .with_title("Recording layers")
                .with_description("Durable layers for one governed recording.")
                .with_mime_type("application/json"),
        ]
}

#[cfg(test)]
mod tests;
