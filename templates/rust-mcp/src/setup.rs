//! The checked setup: identity, capabilities, documents and discovery.
//!
//! `McpServerSetup::new` checks these declarations once at startup. Every
//! descriptor is built from a typed address, so a declared URI always parses.

use std::sync::LazyLock;

use rmcp::model::{Implementation, Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceScheme, ResourceTemplateUri};

use crate::contract::{GlossaryDocument, GlossaryResource, GlossaryScope, SCHEME, TERM_TEMPLATE};

pub static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("glossary"));

pub static SERVER_SETUP: LazyLock<McpServerSetup<GlossaryContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("declared Glossary MCP setup"));

pub struct GlossaryContract;

impl McpServerContract for GlossaryContract {
    type Scope = GlossaryScope;
    type Resource = GlossaryResource;

    fn slug() -> ServerSlug {
        ServerSlug::new("glossary").expect("declared slug")
    }

    fn scheme() -> ResourceScheme {
        ResourceScheme::new(SCHEME).expect("declared scheme")
    }

    fn scopes() -> &'static [GlossaryScope] {
        GlossaryScope::ALL
    }

    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }

    fn server_config() -> ServerConfig {
        let mut config = ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .enable_completions()
                .build(),
        );
        config.server_info = Implementation::new("glossary", env!("CARGO_PKG_VERSION"));
        config.instructions = Some(
            "Glossary of Veoveo hosting terms. Call `define` with a term, or read \
             glossary://terms and glossary://term/{term_id}."
                .into(),
        );
        config
    }

    fn resources() -> Result<Vec<McpResource<GlossaryResource>>, McpSetupError> {
        let mut resources = vec![
            McpResource::new(GlossaryResource::Terms, |uri| {
                Resource::new(uri, "terms")
                    .with_title("Glossary terms")
                    .with_description("Every term with its title and resource URI.")
                    .with_mime_type("application/json")
            })?,
            McpResource::new(GlossaryResource::Docs, |uri| {
                Resource::new(uri, "docs")
                    .with_title("Server documents")
                    .with_description("Index of the crate documents embedded at build time.")
                    .with_mime_type("application/json")
            })?,
            McpResource::new(GlossaryResource::Contract, |uri| {
                Resource::new(uri, "contract")
                    .with_title("Contract declaration")
                    .with_description("Contract revision and compliance of this server.")
                    .with_mime_type("application/json")
            })?,
        ];
        for doc in SERVER_DOCS.iter() {
            let address = GlossaryResource::Document(
                GlossaryDocument::parse(doc.id).map_err(|_| McpSetupError::InvalidDocument)?,
            );
            resources.push(McpResource::new(address, |uri| {
                Resource::new(uri, doc.title)
                    .with_title(doc.title)
                    .with_description("Crate document embedded at build time.")
                    .with_mime_type("text/markdown")
            })?);
        }
        Ok(resources)
    }

    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        let term =
            ResourceTemplateUri::new(TERM_TEMPLATE).map_err(|_| McpSetupError::InvalidTemplate)?;
        let docs =
            veoveo_mcp_contract::docs::knowledge_extension::docs::member_template(&Self::scheme());
        Ok(vec![
            McpResourceTemplate::new(term, |uri| {
                ResourceTemplate::new(uri, "term")
                    .with_title("Glossary term")
                    .with_description("One term's definition and related terms.")
                    .with_mime_type("application/json")
            })?,
            McpResourceTemplate::new(docs, |uri| {
                ResourceTemplate::new(uri, "doc")
                    .with_title("Server document")
                    .with_description("Embedded crate document body.")
                    .with_mime_type("text/markdown")
            })?,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_passes_its_startup_checks() {
        let setup = McpServerSetup::<GlossaryContract>::new().unwrap();
        assert_eq!(setup.resources().len(), 5);
        assert_eq!(setup.resource_templates().len(), 2);
    }
}
