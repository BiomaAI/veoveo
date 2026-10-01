//! Typed hosted declarations checked before connecting services.
use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use std::sync::LazyLock;
use veoveo_artifact_mcp::contract::*;
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceScheme, ResourceTemplateUri};

/// The crate documents embedded at build time and served under the well-known
/// surface: `artifact://docs`, `artifact://docs/{doc_id}`,
/// `artifact://contract`, and the administrative `admin/docs` routes
/// (contract C18-C21).
pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("artifact"));

pub(super) struct ArtifactContract;
pub(super) static SERVER_SETUP: LazyLock<McpServerSetup<ArtifactContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("Artifact MCP declaration"));
impl McpServerContract for ArtifactContract {
    type Scope = ArtifactScope;
    type Resource = ArtifactResource;
    fn slug() -> ServerSlug {
        ServerSlug::new("artifact").expect("declared slug")
    }
    fn scheme() -> ResourceScheme {
        ResourceScheme::new("artifact").expect("declared scheme")
    }
    fn scopes() -> &'static [ArtifactScope] {
        &[]
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        let mut info = ServerConfig::default();
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new("artifact", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Find and share artifacts. Each artifact://{artifact_id} URI names one immutable artifact. Share with named users or groups through grants. Anyone-with-link sharing needs the artifact to be marked releasable, and links expire."
                .to_owned(),
        );
        info
    }
    fn resources() -> Result<Vec<McpResource<ArtifactResource>>, McpSetupError> {
        well_known_resources()
            .into_iter()
            .map(|descriptor| {
                let address = ArtifactResource::parse(&descriptor.uri)
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

/// Well-known surface resources (contract C18, C19). The first
/// `list_resources` page serves these for every authenticated identity.
fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![
        Resource::new(INDEX_URI, "artifact-index").with_mime_type("application/json"),
        veoveo_mcp_apps_extension::app_resource(LIBRARY_APP_URI, "library")
            .with_title("Library")
            .with_description("Governed artifact discovery, inspection, release, and sharing."),
        Resource::new(DOCS_URI, "artifact-docs")
            .with_title("Server documents")
            .with_description("Index of the crate documents embedded at build time.")
            .with_mime_type("application/json"),
    ];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(
                doc_uri(doc.id.try_into().expect("embedded Artifact document")).to_string(),
                doc.title,
            )
            .with_title(doc.title)
            .with_description("Crate document embedded at build time.")
            .with_mime_type("text/markdown"),
        );
    }
    resources.push(
        Resource::new(CONTRACT_URI, "artifact-contract")
            .with_title("Contract declaration")
            .with_description(
                "Machine-readable contract revision, compliance, and capability inventory.",
            )
            .with_mime_type("application/json"),
    );
    resources
}

/// Templates served by `list_resource_templates`.
fn resource_templates() -> Vec<ResourceTemplate> {
    let mut templates = vec![
        ResourceTemplate::new(DOC_TEMPLATE, "artifact-doc")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        ResourceTemplate::new(ARTIFACT_TEMPLATE, "artifact")
            .with_title("Artifact content")
            .with_description("Immutable artifact occurrence bytes.")
            .with_mime_type("application/octet-stream"),
        ResourceTemplate::new(METADATA_TEMPLATE, "artifact-metadata")
            .with_title("Artifact metadata")
            .with_description("Artifact metadata with source provenance and read access.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(GRANTS_TEMPLATE, "artifact-grants")
            .with_title("Artifact grants")
            .with_description("Administrative artifact access-control entries.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(INDEX_TEMPLATE, "artifact-index-page")
            .with_title("Artifact metadata page")
            .with_description(
                "Up to 100 readable metadata resource links with a continuation cursor.",
            )
            .with_mime_type("application/json"),
    ];
    let metadata = templates
        .iter_mut()
        .find(|template| template.uri_template == METADATA_TEMPLATE)
        .expect("metadata template");
    veoveo_mcp_knowledge_extension::server::attach_collection(
        metadata,
        &veoveo_artifact_mcp::knowledge::collection(),
    );
    templates
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_surface_preserves_app_metadata_and_matches_every_template() {
        let setup = &*SERVER_SETUP;
        assert_eq!(setup.resources().len(), 6);
        assert_eq!(setup.resource_templates().len(), 5);
        assert!(setup.scope_names().is_empty());
        let member = setup
            .resource_templates()
            .iter()
            .find(|template| template.template().as_str() == METADATA_TEMPLATE)
            .unwrap();
        let declaration: veoveo_mcp_knowledge_extension::CollectionDescriptor =
            serde_json::from_value(
                member.descriptor().meta.as_ref().unwrap()
                    [veoveo_mcp_knowledge_extension::EXTENSION_ID]
                    .clone(),
            )
            .unwrap();
        assert_eq!(declaration, veoveo_artifact_mcp::knowledge::collection());
        let app = setup
            .resources()
            .iter()
            .find(|r| r.address() == &ArtifactResource::LibraryApp)
            .unwrap();
        assert_eq!(
            app.descriptor(),
            &veoveo_mcp_apps_extension::app_resource(LIBRARY_APP_URI, "library")
                .with_title("Library")
                .with_description("Governed artifact discovery, inspection, release, and sharing.")
        );
        let id = ArtifactId::new();
        for template in setup.resource_templates() {
            let uri = template
                .template()
                .as_str()
                .replace("{artifact_id}", &id.to_string())
                .replace("{?cursor}", "")
                .replace("{doc_id}", "agents");
            assert_eq!(
                ArtifactResource::parse(&uri).unwrap().to_uri().as_str(),
                uri
            );
        }
        assert!(
            ArtifactScope::try_from(&veoveo_types::ScopeName::new("external:read").unwrap())
                .is_err()
        );
    }
}
