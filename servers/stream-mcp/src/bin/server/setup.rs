//! Checked declarations assembled before Store access or task recovery.
use super::tasks::SERVER_SLUG;
use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use std::sync::LazyLock;
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_stream_mcp::{
    catalog::PipelineCatalog,
    contract::{StreamResource, StreamScope},
    uris,
};
use veoveo_types::{ResourceScheme, ResourceTemplateUri};

pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("stream"));
pub(super) struct StreamContract;
pub(super) static SERVER_SETUP: LazyLock<McpServerSetup<StreamContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("Stream MCP contract setup"));
impl McpServerContract for StreamContract {
    type Scope = StreamScope;
    type Resource = StreamResource;
    fn slug() -> ServerSlug {
        ServerSlug::new(SERVER_SLUG).expect("declared Stream slug")
    }
    fn scheme() -> ResourceScheme {
        uris::SCHEME.clone()
    }
    fn scopes() -> &'static [StreamScope] {
        &[]
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_info()
    }
    fn resources() -> Result<Vec<McpResource<StreamResource>>, McpSetupError> {
        resources()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        resource_templates()
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
    info.server_info = rmcp::model::Implementation::new(SERVER_SLUG, env!("CARGO_PKG_VERSION"));
    info.instructions = Some(
            "Run configured GStreamer pipelines on live video or recordings. Find pipelines and models at stream://pipelines and stream://models. Use `run_recording` to process a recording. Live sessions publish stream://session resources as results arrive, without waiting for Recording Hub."
                .to_owned(),
        );
    info
}

fn resources() -> Result<Vec<McpResource<StreamResource>>, McpSetupError> {
    let mut resources = vec![
        veoveo_mcp_apps_extension::app_resource(uris::LIVE_APP_URI, "stream-live-app")
            .with_title("Live Monitor")
            .with_description("Live encoded video and typed Stream pipeline overlays."),
        Resource::new(uris::DOCS_URI, "stream docs")
            .with_title("Server documents")
            .with_description("Index of the crate documents embedded at build time.")
            .with_mime_type("application/json"),
        Resource::new(uris::CONTRACT_URI, "stream contract")
            .with_title("Contract declaration")
            .with_description(
                "Machine-readable contract revision, compliance, and capability inventory.",
            )
            .with_mime_type("application/json"),
        Resource::new(uris::PIPELINES_URI, "stream pipelines")
            .with_title("Stream pipelines")
            .with_description("Operator-admitted GStreamer pipeline catalog.")
            .with_mime_type("application/json"),
        Resource::new(uris::MODELS_URI, "stream models")
            .with_title("Stream models")
            .with_description("Immutable model catalog without private filesystem details.")
            .with_mime_type("application/json"),
        Resource::new(uris::RUNS_URI, "stream recording runs")
            .with_title("Stream recording runs")
            .with_description("Authorized durable recording-run index.")
            .with_mime_type("application/json"),
        Resource::new(uris::SESSIONS_URI, "stream live sessions")
            .with_title("Stream live sessions")
            .with_description(
                "Work-Context-readable live pipeline sessions with owner-scoped control.",
            )
            .with_mime_type("application/json"),
    ];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(
                uris::doc_uri(
                    veoveo_stream_mcp::contract::StreamDocument::parse(doc.id)
                        .expect("declared server document"),
                ),
                doc.title,
            )
            .with_title(doc.title)
            .with_description("Crate document embedded at build time.")
            .with_mime_type("text/markdown"),
        );
    }
    resources
        .into_iter()
        .map(|descriptor| {
            let address = StreamResource::parse(&descriptor.uri)
                .map_err(|_| McpSetupError::InvalidResource)?;
            McpResource::new(address, |_| descriptor)
        })
        .collect()
}

fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
    let templates = vec![
        ResourceTemplate::new(uris::DOC_TEMPLATE, "doc")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        ResourceTemplate::new(uris::PIPELINE_TEMPLATE, "pipeline")
            .with_title("Stream pipeline")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::MODEL_TEMPLATE, "model")
            .with_title("Stream model")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::RUNS_PAGE_TEMPLATE, "recording run page")
            .with_title("Stream recording runs")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::SESSIONS_PAGE_TEMPLATE, "live session page")
            .with_title("Stream live sessions")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::RUN_TEMPLATE, "run")
            .with_title("Stream recording run")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::RUN_RESULTS_TEMPLATE, "run results")
            .with_title("Stream recording-run results")
            .with_mime_type("application/vnd.veoveo.stream-results+json"),
        ResourceTemplate::new(uris::SESSION_TEMPLATE, "live session")
            .with_title("Stream live session")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::SESSION_RESULTS_TEMPLATE, "live session results")
            .with_title("Stream live session results")
            .with_mime_type("application/vnd.veoveo.stream-live-results+json"),
        ResourceTemplate::new(uris::SESSION_PREVIEW_TEMPLATE, "live session preview")
            .with_title("Stream live encoded preview")
            .with_mime_type("application/vnd.veoveo.stream-live-preview+json"),
        ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact").with_title("Stream artifact"),
    ];
    templates
        .into_iter()
        .map(|descriptor| {
            let template = ResourceTemplateUri::new(descriptor.uri_template.as_str())
                .map_err(|_| McpSetupError::InvalidResource)?;
            McpResourceTemplate::new(template, |_| descriptor)
        })
        .collect()
}

pub(super) fn catalog_resources(catalog: &PipelineCatalog) -> Result<Vec<Resource>, McpSetupError> {
    let mut resources = Vec::new();
    for pipeline in catalog.pipeline_views() {
        let name = format!("pipeline {}", pipeline.id());
        let address = StreamResource::Pipeline(pipeline.uri);
        resources.push(
            McpResource::new(address, |uri| {
                Resource::new(uri, name)
                    .with_title(pipeline.title)
                    .with_description(pipeline.description)
                    .with_mime_type("application/json")
            })?
            .descriptor()
            .clone(),
        );
    }
    for model in catalog.model_views() {
        let name = format!("model {}", model.id());
        let address = StreamResource::Model(model.uri);
        resources.push(
            McpResource::new(address, |uri| {
                Resource::new(uri, name)
                    .with_title(model.title)
                    .with_description(model.description)
                    .with_mime_type("application/json")
            })?
            .descriptor()
            .clone(),
        );
    }
    Ok(resources)
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod tests;
