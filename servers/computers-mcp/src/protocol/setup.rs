//! Checked hosted declarations composed from the Computers public contract.
use rmcp::model::*;
use std::sync::LazyLock;
use veoveo_computers_contract::*;
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceScheme, ResourceTemplateUri};

pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("computers"));
pub(crate) struct ComputersContract;
pub(crate) static SERVER_SETUP: LazyLock<McpServerSetup<ComputersContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("declared Computers setup"));
impl McpServerContract for ComputersContract {
    type Scope = ComputerScope;
    type Resource = ComputerResource;
    fn slug() -> ServerSlug {
        ServerSlug::new("computers").expect("declared slug")
    }
    fn scheme() -> ResourceScheme {
        ResourceScheme::new("computer").expect("declared resource scheme")
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
        resource_catalog()
            .into_iter()
            .map(|descriptor| {
                let address = ComputerResource::parse(&descriptor.uri)
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
    let mut capabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_resources()
        .enable_resources_subscribe()
        .enable_prompts()
        .enable_completions()
        .build();
    capabilities
        .extensions
        .get_or_insert_default()
        .insert(TASKS_EXTENSION_ID.into(), JsonObject::new());
    let mut info = ServerConfig::default();
    info.capabilities = capabilities;
    info.server_info = Implementation::new("computers", env!("CARGO_PKG_VERSION"));
    info.instructions = Some("Start by reading computer://computers to see your Computers, whether they are available, and which actions you may take. Call lifecycle tools as MCP Tasks with a requestId you reuse on retries. Disconnecting leaves work running; Stop ends processes and keeps the home directory. An operation marked Needs recovery had an uncertain outcome and will not run again automatically; check the Computer before retrying.".into());
    info
}
fn resource_catalog() -> Vec<Resource> {
    let mut resources = [
        (
            COMPUTERS_URI,
            "computers",
            "Your Computers and current permitted actions",
        ),
        (
            "computer://docs",
            "docs",
            "Computer capability documentation",
        ),
        (
            "computer://contract",
            "contract",
            "Implemented server contract",
        ),
    ]
    .into_iter()
    .map(|(uri, name, description)| {
        Resource::new(uri, name)
            .with_description(description)
            .with_mime_type("application/json")
    })
    .collect::<Vec<_>>();
    resources.extend(SERVER_DOCS.iter().map(|doc| {
        let address =
            ComputerResource::Document(ComputerDocument::parse(doc.id).expect("embedded document"));
        Resource::new(address.to_uri().to_string(), doc.id)
            .with_title(doc.title)
            .with_description("Crate document embedded at build time.")
            .with_mime_type("text/markdown")
    }));
    resources
}
fn resource_templates() -> Vec<ResourceTemplate> {
    [
        (COMPUTER_TEMPLATE, "computer", "One private Computer"),
        (
            FileTransferResultUri::TEMPLATE,
            "file-transfer-result",
            "A file transfer's verified result and its Artifact",
        ),
        (
            MAINTENANCE_TEMPLATE,
            "computer-maintenance",
            "Environment updates you can apply, and the progress of an active one",
        ),
        (
            AUTOMATION_TEMPLATE,
            "computer-automation",
            "Live named automation grants for your Computer",
        ),
        (
            GRANT_TEMPLATE,
            "automation-grant",
            "One automation grant, including whether it was revoked or expired",
        ),
        (
            ExecutionResultUri::TEMPLATE,
            "execution-result",
            "A command's exit status and its output Artifacts",
        ),
        (
            ACCESS_TEMPLATE,
            "computer-access",
            "Outstanding access grants for your Computer",
        ),
        (
            PAGE_TEMPLATE,
            "computer-page",
            "Next page of your Computers",
        ),
        (DOC_TEMPLATE, "document", "Embedded server documentation"),
    ]
    .into_iter()
    .map(|(uri, name, description)| {
        ResourceTemplate::new(uri, name)
            .with_description(description)
            .with_mime_type(if uri == DOC_TEMPLATE {
                "text/markdown"
            } else {
                "application/json"
            })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn checked_setup_preserves_capabilities_and_admits_every_template_expansion() {
        let setup = &*SERVER_SETUP;
        assert_eq!(setup.resources().len(), 5);
        assert_eq!(setup.resource_templates().len(), 9);
        let capabilities = &setup.server_config().capabilities;
        assert_eq!(
            capabilities.resources.as_ref().unwrap().subscribe,
            Some(true)
        );
        assert!(
            capabilities
                .extensions
                .as_ref()
                .unwrap()
                .contains_key(TASKS_EXTENSION_ID)
        );
        assert!(capabilities.tools.is_some());
        assert!(capabilities.prompts.is_some());
        assert!(capabilities.completions.is_some());
        let computer = ComputerId::new();
        let variables = BTreeMap::from([
            ("computer_id".to_owned(), computer.to_string()),
            ("grant_id".to_owned(), AutomationGrantId::new().to_string()),
            ("execution_id".to_owned(), ExecutionId::new().to_string()),
            ("transfer_id".to_owned(), FileTransferId::new().to_string()),
            ("doc_id".to_owned(), "design".to_owned()),
        ]);
        for template in setup.resource_templates() {
            let uri = template.template().expand_scalars(&variables).unwrap();
            assert_eq!(ComputerResource::parse(uri.as_str()).unwrap().to_uri(), uri);
            if template.template().as_str() == PAGE_TEMPLATE {
                assert_eq!(
                    ComputerResource::parse(uri.as_str()).unwrap(),
                    ComputerResource::Collection(None)
                );
                let mut paged = variables.clone();
                paged.insert("after".into(), computer.to_string());
                let uri = template.template().expand_scalars(&paged).unwrap();
                assert_eq!(
                    ComputerResource::parse(uri.as_str()).unwrap(),
                    ComputerResource::Collection(Some(computer))
                );
            }
        }
    }
}
