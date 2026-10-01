use crate::contract::{KnowledgeResource, KnowledgeScope};
use rmcp::model::{Implementation, Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use std::sync::LazyLock;
use veoveo_mcp_contract::{
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceScheme, ResourceTemplateUri, ServerSlug};

pub struct KnowledgeContract;
pub static DOCUMENTS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("knowledge"));
pub static SETUP: LazyLock<McpServerSetup<KnowledgeContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("Knowledge contract setup"));
impl McpServerContract for KnowledgeContract {
    type Scope = KnowledgeScope;
    type Resource = KnowledgeResource;
    fn slug() -> ServerSlug {
        "knowledge".parse().expect("declared slug")
    }
    fn scheme() -> ResourceScheme {
        "knowledge".parse().expect("declared scheme")
    }
    fn scopes() -> &'static [KnowledgeScope] {
        KnowledgeScope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &DOCUMENTS
    }
    fn server_config() -> ServerConfig {
        let mut config = ServerConfig::default();
        config.server_info = Implementation::new("knowledge", env!("CARGO_PKG_VERSION"));
        config.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        veoveo_mcp_knowledge_extension::server::declare(&mut config.capabilities);
        config.instructions = Some("Search approved collections for reusable findings. Read result resource links from their owning servers for current content. Source servers decide access. Embed creates vectors in the installation's declared embedding space.".into());
        config
    }
    fn resources() -> Result<Vec<McpResource<KnowledgeResource>>, McpSetupError> {
        let mut resources = vec![];
        for (address, name, mime) in [
            (
                KnowledgeResource::Sources { after: None },
                "Knowledge sources",
                "application/json",
            ),
            (
                KnowledgeResource::Docs { after: None },
                "Documents",
                "application/json",
            ),
            (KnowledgeResource::Contract, "Contract", "application/json"),
        ] {
            resources.push(McpResource::new(address, |uri| {
                Resource::new(uri, name).with_mime_type(mime)
            })?);
        }
        for document in DOCUMENTS.iter() {
            resources.push(McpResource::new(
                KnowledgeResource::Document(document.id.parse().expect("document id")),
                |uri| Resource::new(uri, document.title).with_mime_type("text/markdown"),
            )?);
        }
        Ok(resources)
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        [
            ("knowledge://sources{?cursor}", "Knowledge source page"),
            ("knowledge://source/{server}", "Knowledge source"),
            (
                "knowledge://collection/{collection}",
                "Knowledge collection",
            ),
            ("knowledge://docs{?cursor}", "Document page"),
            ("knowledge://docs/{doc_id}", "Document"),
        ]
        .into_iter()
        .map(|(uri, name)| {
            McpResourceTemplate::new(
                ResourceTemplateUri::new(uri).map_err(|_| McpSetupError::InvalidTemplate)?,
                |uri| {
                    let mut template =
                        ResourceTemplate::new(uri, name).with_mime_type(if name == "Document" {
                            "text/markdown"
                        } else {
                            "application/json"
                        });
                    if name == "Document" {
                        veoveo_mcp_knowledge_extension::server::attach_collection(
                            &mut template,
                            &veoveo_mcp_knowledge_extension::docs::collection(
                                &Self::slug(),
                                &Self::scheme(),
                            ),
                        );
                    }
                    template
                },
            )
        })
        .collect()
    }
}
