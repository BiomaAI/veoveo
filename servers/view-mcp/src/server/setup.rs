//! View declarations checked before opening the Store or starting the renderer.
use crate::{
    contract::{ViewDocument, ViewResource, ViewScope},
    uris,
};
use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use std::{collections::BTreeSet, sync::LazyLock};
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceTemplateUri, ScopeName};

pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("view"));
pub(crate) struct ViewContract;
pub(crate) static SERVER_SETUP: LazyLock<McpServerSetup<ViewContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("View MCP contract setup"));
impl McpServerContract for ViewContract {
    type Scope = ViewScope;
    type Resource = ViewResource;
    fn slug() -> ServerSlug {
        ServerSlug::new("view").expect("declared View slug")
    }
    fn scheme() -> ResourceScheme {
        ResourceScheme::new("view").expect("declared View scheme")
    }
    fn scopes() -> &'static [ViewScope] {
        ViewScope::ALL
    }
    fn documents() -> &'static ServerDocs {
        &SERVER_DOCS
    }
    fn server_config() -> ServerConfig {
        server_config()
    }
    fn resources() -> Result<Vec<McpResource<ViewResource>>, McpSetupError> {
        let mut resources = well_known_resources();
        resources.extend([
            json_descriptor(uris::LAYERS, "View layers", "Configured 3D scene layers."),
            json_descriptor(uris::COMPOSITIONS, "Scene compositions", "Owner-scoped immutable governed scene compositions."),
            json_descriptor(uris::VIEWS, "Views", "Owner-scoped camera views."),
            json_descriptor(uris::FRAMES, "Frames", "Owner-scoped captured frames."),
            veoveo_mcp_apps_extension::app_resource(uris::PREVIEW_APP_URI, "view-preview-app")
                .with_title("Preview")
                .with_description("Interactive MCP App that composes camera poses over configured 3D Tiles layers, previews the scene in-browser, and drives the real view lifecycle including task-based capture.")
                .with_icons(vec![rmcp::model::Icon::new(PREVIEW_APP_ICON)]),
        ]);
        resources
            .into_iter()
            .map(|descriptor| {
                let address = ViewResource::parse(&descriptor.uri)
                    .map_err(|_| McpSetupError::InvalidResource)?;
                McpResource::new(address, |_| descriptor)
            })
            .collect()
    }
    fn resource_templates() -> Result<Vec<McpResourceTemplate>, McpSetupError> {
        resource_templates()
            .into_iter()
            .map(|descriptor| {
                let template = ResourceTemplateUri::new(descriptor.uri_template.clone())
                    .map_err(|_| McpSetupError::InvalidTemplate)?;
                McpResourceTemplate::new(template, |_| descriptor)
            })
            .collect()
    }
}

pub(crate) fn visible_resources(grants: &BTreeSet<ScopeName>) -> Vec<Resource> {
    SERVER_SETUP
        .resources()
        .iter()
        .filter(|resource| {
            !matches!(resource.address(), ViewResource::PreviewApp)
                || SERVER_SETUP.has_scope(grants, ViewScope::Capture)
        })
        .map(|resource| resource.descriptor().clone())
        .collect()
}

const PREVIEW_APP_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNCIgaGVpZ2h0PSIyNCIgdmlld0JveD0iMCAwIDI0IDI0IiBmaWxsPSJub25lIiBzdHJva2U9IiM0YTdkZDYiIHN0cm9rZS13aWR0aD0iMiIgc3Ryb2tlLWxpbmVjYXA9InJvdW5kIiBzdHJva2UtbGluZWpvaW49InJvdW5kIj48cGF0aCBkPSJtMTQgMTAgNy0zdjEwbC03LTMiLz48cmVjdCB4PSIyIiB5PSI3IiB3aWR0aD0iMTIiIGhlaWdodD0iMTAiIHJ4PSIyIi8+PC9zdmc+";

fn server_config() -> ServerConfig {
    let mut capabilities = ServerCapabilities::builder()
        .enable_tools()
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
    info.server_info = rmcp::model::Implementation::new("view", env!("CARGO_PKG_VERSION"));
    info.instructions = Some(
            "Render 3D Tiles scenes. Create a scene from a base layer and overlays, then create a view with a camera pose or target. Change the camera with `set_camera`, and call `capture_frame` as an MCP Task with an explicit scene time. A capture returns an image you can show directly, with provenance metadata. Read its view://frame address for the image bytes. The ui://view/preview.html app does the same interactively."
                .to_owned(),
        );
    info
}

fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![json_descriptor(
        uris::DOCS,
        "Server documents",
        "Index of the crate documents embedded at build time.",
    )];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(
                ViewResource::Document(
                    ViewDocument::parse(doc.id).expect("embedded View document"),
                )
                .to_uri()
                .expect("declared document")
                .to_string(),
                doc.title,
            )
            .with_title(doc.title)
            .with_description("Crate document embedded at build time.")
            .with_mime_type("text/markdown"),
        );
    }
    resources.push(json_descriptor(
        uris::CONTRACT,
        "Contract declaration",
        "Machine-readable contract revision, compliance, and capability inventory.",
    ));
    resources
}

/// Every advertised resource template. `list_resource_templates` serves this
/// list and the `view://contract` capability inventory declares it, so the
/// two cannot diverge.
fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(uris::DOC_TEMPLATE, "Server document")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        template(
            uris::LAYER_TEMPLATE,
            "View layer",
            "Configured scene layer.",
        ),
        template(
            uris::COMPOSITION_TEMPLATE,
            "Scene composition",
            "Owner-scoped immutable governed scene composition.",
        ),
        template(uris::VIEW_TEMPLATE, "View", "Owner-scoped camera view."),
        ResourceTemplate::new(uris::FRAME_TEMPLATE, "Frame")
            .with_title("Frame")
            .with_description(
                "Owner-scoped captured PNG or JPEG image; the response supplies its media type.",
            ),
        template(
            uris::VIEW_SCENE_TEMPLATE,
            "View scene",
            "Render-cut manifest for the view's current camera and preview policy.",
        ),
        ResourceTemplate::new(uris::TILE_TEMPLATE, "Preview tile")
            .with_title("Preview tile")
            .with_description("Raw draco GLB tile content from a scene manifest.")
            .with_mime_type("model/gltf-binary"),
    ]
}

fn json_descriptor(uri: &str, title: &str, description: &str) -> Resource {
    Resource::new(uri.to_owned(), title.to_owned())
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}

fn template(uri: &str, title: &str, description: &str) -> ResourceTemplate {
    ResourceTemplate::new(uri, title)
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}

#[cfg(test)]
mod tests;

pub(crate) fn accepted_subscription_filter(
    requested: &rmcp::model::SubscriptionFilter,
) -> Option<rmcp::model::SubscriptionFilter> {
    let mut accepted = rmcp::model::SubscriptionFilter::builder().build();
    accepted.task_ids = requested.task_ids.clone().filter(|ids| !ids.is_empty());
    accepted.resource_subscriptions = requested
        .resource_subscriptions
        .as_ref()
        .map(|uris| {
            uris.iter()
                .filter(|uri| {
                    ViewResource::parse(uri).is_ok_and(|resource| resource.is_subscribable())
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .filter(|uris| !uris.is_empty());
    (accepted.task_ids.is_some() || accepted.resource_subscriptions.is_some()).then_some(accepted)
}
