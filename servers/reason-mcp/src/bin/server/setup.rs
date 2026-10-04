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
use veoveo_reason_mcp::{
    catalog::PipelineCatalog,
    contract::{FindingCollection, FindingResource, ReasonDocument, ReasonResource, ReasonScope},
    uris,
};
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceTemplateUri};

pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("reason"));
pub(super) struct ReasonContract;
pub(super) static SERVER_SETUP: LazyLock<McpServerSetup<ReasonContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("Reason MCP contract setup"));
impl McpServerContract for ReasonContract {
    type Scope = ReasonScope;
    type Resource = ReasonResource;
    fn slug() -> ServerSlug {
        ServerSlug::parse(SERVER_SLUG).expect("declared Reason slug")
    }
    fn scheme() -> ResourceScheme {
        uris::SCHEME.clone()
    }
    fn scopes() -> &'static [ReasonScope] {
        ReasonScope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_info()
    }
    fn resources() -> Result<Vec<McpResource<ReasonResource>>, McpSetupError> {
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
            "Reasoning over Rerun recordings. Find models and pipelines at reason://models and reason://pipelines. Call `analyze_recording` as an MCP Task with recording:// references, a timeline range, and one reasoning task. To have events cite track IDs, also pass a completed Stream results artifact. Each analysis publishes a reason://analysis resource and artifacts. Results are the model's own reasoning, not calibrated detector output."
                .to_owned(),
        );
    info
}

fn resources() -> Result<Vec<McpResource<ReasonResource>>, McpSetupError> {
    let mut resources = vec![
        veoveo_mcp_apps_extension::app_resource(uris::ANALYSES_APP_URI, "analyses")
            .with_title("Analyses")
            .with_description("Governed video reasoning pipelines, models, runs, and results."),
        Resource::new(uris::DOCS_URI, "reason docs")
            .with_title("Server documents")
            .with_description("Index of the crate documents embedded at build time.")
            .with_mime_type("application/json"),
        Resource::new(uris::CONTRACT_URI, "reason contract")
            .with_title("Contract declaration")
            .with_description(
                "Machine-readable contract revision, compliance, and capability inventory.",
            )
            .with_mime_type("application/json"),
        Resource::new(uris::PIPELINES_URI, "reason pipelines")
            .with_title("Reason pipelines")
            .with_description("Immutable reasoning pipeline catalog.")
            .with_mime_type("application/json"),
        Resource::new(uris::MODELS_URI, "reason models")
            .with_title("Reason models")
            .with_description("Immutable model catalog without private filesystem details.")
            .with_mime_type("application/json"),
        Resource::new(uris::ANALYSES_URI, "reason analyses")
            .with_title("Reason analyses")
            .with_description("Authorized durable analysis index.")
            .with_mime_type("application/json"),
    ];
    for collection in FindingCollection::ALL {
        resources.push(
            Resource::new(
                FindingResource::root(collection)
                    .to_uri()
                    .map_err(|_| McpSetupError::InvalidResource)?
                    .to_string(),
                format!("completed {}", collection.segment()),
            )
            .with_title(format!("Completed {}", collection.segment()))
            .with_description("Readable completed findings, with cursor pagination.")
            .with_mime_type("application/json"),
        );
    }
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(
                uris::doc_uri(ReasonDocument::parse(doc.id).expect("embedded Reason document")),
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
            let address = ReasonResource::parse(&descriptor.uri)
                .map_err(|_| McpSetupError::InvalidResource)?;
            McpResource::new(address, |_| descriptor)
        })
        .collect()
}

fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
    let mut templates = vec![
        ResourceTemplate::new(uris::DOC_TEMPLATE, "doc")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        ResourceTemplate::new(uris::PIPELINE_TEMPLATE, "pipeline")
            .with_title("Reason pipeline")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::MODEL_TEMPLATE, "model")
            .with_title("Reason model")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::ANALYSES_PAGE_TEMPLATE, "analyses page")
            .with_title("Reason analyses page")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::ANALYSIS_TEMPLATE, "analysis")
            .with_title("Reason analysis")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::RESULTS_TEMPLATE, "analysis results")
            .with_title("Reason analysis results")
            .with_mime_type("application/vnd.veoveo.reason-results+json"),
        ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact").with_title("Reason artifact"),
    ];
    for collection in FindingCollection::ALL {
        templates.push(
            ResourceTemplate::new(
                collection.page_template(),
                format!("{} pages", collection.segment()),
            )
            .with_mime_type("application/json"),
        );
        let mut member = ResourceTemplate::new(
            collection.member_template(),
            format!("completed {}", collection.segment()),
        )
        .with_title(format!("Completed {} summary", collection.segment()))
        .with_mime_type("application/json");
        veoveo_mcp_knowledge_extension::server::attach_collection(
            &mut member,
            &collection.descriptor(),
        );
        templates.push(member);
    }
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
        let address = ReasonResource::Pipeline(pipeline.uri);
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
        let address = ReasonResource::Model(model.uri);
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
