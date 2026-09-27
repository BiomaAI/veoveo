//! The complete discovery surface is available without database or Artifact services.
use super::{BATCH_ARTIFACT_MIME, SERVER_DOCS};
use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities};
use veoveo_frames_mcp::{
    contract::{FrameTaskUsageUri, FrameUsageIndexUri, FrameWorldsUri},
    uris,
};

pub(super) fn capabilities() -> ServerCapabilities {
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

pub(super) fn resources() -> Vec<Resource> {
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
            Resource::new(uris::doc_uri(doc.id), doc.title)
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
pub(super) fn resource_templates() -> Vec<ResourceTemplate> {
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
        ResourceTemplate::new(uris::OPERATION_TEMPLATE, "operation")
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
        expected.extend(SERVER_DOCS.iter().map(|doc| uris::doc_uri(doc.id)));
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
