//! The complete discovery surface is available without database or Artifact services.
use super::BATCH_ARTIFACT_MIME;
use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use veoveo_frames_mcp::{
    contract::{FrameOperationUri, FrameTaskUsageUri, FrameUsageIndexUri, FrameWorldsUri},
    uris,
};

fn capabilities() -> ServerCapabilities {
    let mut caps: ServerCapabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_prompts()
        .enable_resources()
        .enable_resources_subscribe()
        .enable_completions()
        .build();
    veoveo_mcp_apps_extension::extend_capabilities(&mut caps);
    caps.extensions.get_or_insert_default().insert(
        rmcp::model::TASKS_EXTENSION_ID.to_owned(),
        rmcp::model::JsonObject::new(),
    );
    caps
}

fn resources() -> Vec<Resource> {
    let mut resources = well_known_resources();
    resources.extend([
        veoveo_mcp_apps_extension::app_resource(uris::WORKSPACE_APP_URI, "workspace")
            .with_title("Frame Editor")
            .with_description("Author frame worlds and run bounded coordinate transforms."),
        Resource::new(FrameWorldsUri::ROOT, "worlds")
            .with_title("Frame worlds")
            .with_description("Visible authored worlds in pages of at most 100.")
            .with_mime_type("application/json"),
        Resource::new(FrameUsageIndexUri::ROOT, "usage")
            .with_title("Frames usage ledger")
            .with_description("Caller-owned task usage resources in pages of at most 100.")
            .with_mime_type("application/json"),
    ]);
    resources.sort_by(|left, right| left.uri.cmp(&right.uri));
    resources
}

/// Well-known surface resources (contract C18, C19). `list_resources` serves
/// these for every authenticated identity and `capability_inventory` declares
/// them at `frames://contract`, so the two cannot diverge.
fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![
        Resource::new(uris::DOCS_URI, "docs")
            .with_title("Server documents")
            .with_description("Index of the crate documents embedded at build time.")
            .with_mime_type("application/json"),
    ];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(
                FramesResource::Document(FramesDocument::parse(doc.id).expect("embedded document"))
                    .to_uri()
                    .expect("typed document URI")
                    .to_string(),
                doc.title,
            )
            .with_title(doc.title)
            .with_description("Crate document embedded at build time.")
            .with_mime_type("text/markdown"),
        );
    }
    resources.push(
        Resource::new(uris::CONTRACT_URI, "contract")
            .with_title("Contract declaration")
            .with_description(
                "Machine-readable contract revision, compliance, and capability inventory.",
            )
            .with_mime_type("application/json"),
    );
    resources
}

/// Templates served by `list_resource_templates` and declared in the
/// `frames://contract` capability inventory.
fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(FrameWorldsUri::TEMPLATE, "world pages")
            .with_title("Frame worlds")
            .with_description("Visible authored worlds in pages of at most 100.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::DOC_TEMPLATE, "doc")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        ResourceTemplate::new(uris::WORLD_TEMPLATE, "world")
            .with_title("Frame world")
            .with_description("Mutable world head and authoring metadata.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::WORLD_REVISION_TEMPLATE, "world revision")
            .with_title("Frame world revision")
            .with_description("Immutable complete rooted frame tree.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::WORLD_FRAME_TEMPLATE, "world frame")
            .with_title("Revision-scoped world frame")
            .with_description("Typed frame node inside one immutable world revision.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(FrameOperationUri::TEMPLATE, "operation")
            .with_title("Coordinate operation")
            .with_description("Recorded operation provenance.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
            .with_title("Frames artifact")
            .with_description("Shared-plane immutable Frames artifact.")
            .with_mime_type(BATCH_ARTIFACT_MIME),
        ResourceTemplate::new(FrameUsageIndexUri::TEMPLATE, "usage pages")
            .with_title("Frames usage ledger")
            .with_description("Caller-owned task usage resources in pages of at most 100.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(FrameTaskUsageUri::TEMPLATE, "usage")
            .with_title("Frames task usage")
            .with_description("Usage rows for one Frames task.")
            .with_mime_type("application/json"),
    ]
}

use std::sync::LazyLock;
use veoveo_frames_mcp::contract::{FramesDocument, FramesResource, FramesScope};
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceTemplateUri};

pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("frames"));
pub(super) struct FramesContract;
pub(super) static SERVER_SETUP: LazyLock<McpServerSetup<FramesContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("declared Frames MCP setup"));
impl McpServerContract for FramesContract {
    type Scope = FramesScope;
    type Resource = FramesResource;
    fn slug() -> ServerSlug {
        ServerSlug::parse("frames").expect("declared slug")
    }
    fn scheme() -> ResourceScheme {
        uris::SCHEME.clone()
    }
    fn scopes() -> &'static [Self::Scope] {
        Self::Scope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_info()
    }
    fn resources() -> Result<Vec<McpResource<Self::Resource>>, McpSetupError> {
        resources()
            .into_iter()
            .map(|descriptor| {
                let address = FramesResource::parse(&descriptor.uri)
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
    let mut info = ServerConfig::default();
    info.capabilities = capabilities();
    info.server_info = rmcp::model::Implementation::new("frames", env!("CARGO_PKG_VERSION"));
    info.instructions = Some(
        "Coordinate frames and frame worlds. Create a world, publish its frame tree as a \
         revision, and reference frames by their revision URIs in sessions and recordings. \
         Use `convert_frame` for small conversions and `batch_transform` as an MCP Task for \
         bulk conversion with artifact output."
            .into(),
    );
    info
}

#[cfg(test)]
mod setup_tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn checked_setup_qualifies_templates_capabilities_and_app_metadata() {
        let setup = &*SERVER_SETUP;
        assert_eq!(setup.resources().len(), 7);
        assert_eq!(setup.resource_templates().len(), 9);
        assert!(setup.scope_names().is_empty());
        assert!(
            FramesScope::try_from(&veoveo_types::ScopeName::parse("external:read").unwrap())
                .is_err()
        );
        let capabilities = &setup.server_config().capabilities;
        assert!(
            capabilities
                .resources
                .as_ref()
                .unwrap()
                .subscribe
                .unwrap_or(false)
        );
        assert!(
            !capabilities
                .resources
                .as_ref()
                .unwrap()
                .list_changed
                .unwrap_or(false)
        );
        assert!(
            capabilities
                .extensions
                .as_ref()
                .unwrap()
                .contains_key(rmcp::model::TASKS_EXTENSION_ID)
        );
        let app = setup
            .resources()
            .iter()
            .find(|resource| resource.descriptor().uri == uris::WORKSPACE_APP_URI)
            .unwrap();
        let original = resources()
            .into_iter()
            .find(|resource| resource.uri == uris::WORKSPACE_APP_URI)
            .unwrap();
        assert_eq!(
            serde_json::to_value(app.descriptor()).unwrap(),
            serde_json::to_value(original).unwrap()
        );
        let task = veoveo_types::TaskId::new();
        let artifact = veoveo_artifact_contract::ArtifactId::new();
        let variables = BTreeMap::from([
            ("doc_id".to_owned(), "agents".to_owned()),
            ("task_id".to_owned(), task.to_string()),
            ("artifact_id".to_owned(), artifact.to_string()),
            ("world_id".to_owned(), "survey".to_owned()),
            ("revision_id".to_owned(), "revision-1".to_owned()),
            ("frame_id".to_owned(), "camera".to_owned()),
            ("operation_id".to_owned(), "conversion-1".to_owned()),
        ]);
        for template in setup.resource_templates() {
            let uri = template.template().expand_scalars(&variables).unwrap();
            let address = FramesResource::parse(uri.as_str()).unwrap();
            assert_eq!(address.to_uri().unwrap(), uri);
            if template.template().variables().any(|name| name == "cursor") {
                let mut paged = variables.clone();
                let cursor = if template.template().as_str() == FrameWorldsUri::TEMPLATE {
                    veoveo_frames_mcp::contract::FrameWorldCursor::new(
                        &veoveo_frames_mcp::contract::FrameWorldId::parse("survey").unwrap(),
                    )
                    .as_str()
                    .to_owned()
                } else {
                    veoveo_frames_mcp::contract::FrameUsageCursor::new(task)
                        .unwrap()
                        .as_str()
                        .to_owned()
                };
                paged.insert("cursor".into(), cursor);
                let uri = template.template().expand_scalars(&paged).unwrap();
                assert_eq!(
                    FramesResource::parse(uri.as_str())
                        .unwrap()
                        .to_uri()
                        .unwrap(),
                    uri
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_is_a_fixed_surface_without_service_inputs() {
        let resources = resources();
        let capabilities = capabilities().resources.unwrap();
        assert!(!capabilities.list_changed.unwrap_or(false));
        assert_eq!(capabilities.subscribe, Some(true));
        let mut expected = vec![
            FrameWorldsUri::ROOT.to_owned(),
            FrameUsageIndexUri::ROOT.to_owned(),
            uris::WORKSPACE_APP_URI.to_owned(),
            uris::DOCS_URI.to_owned(),
            uris::CONTRACT_URI.to_owned(),
        ];
        expected.extend(SERVER_DOCS.iter().map(|doc| {
            FramesResource::Document(FramesDocument::parse(doc.id).expect("embedded document"))
                .to_uri()
                .expect("typed document URI")
                .to_string()
        }));
        expected.sort();
        assert_eq!(
            resources
                .iter()
                .map(|resource| resource.uri.clone())
                .collect::<Vec<_>>(),
            expected
        );
        assert!(
            resources
                .iter()
                .all(|resource| resource.mime_type.is_some())
        );
        assert!(
            resource_templates()
                .iter()
                .any(|template| template.uri_template == FrameWorldsUri::TEMPLATE)
        );
        assert!(
            resource_templates()
                .iter()
                .all(|template| template.mime_type.is_some())
        );
    }
}
