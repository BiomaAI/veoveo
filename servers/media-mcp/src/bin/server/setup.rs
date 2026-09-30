//! Checked MCP startup, fixed discovery and templates.
use rmcp::model::{Resource, ResourceTemplate, ServerCapabilities, ServerConfig};
use std::sync::LazyLock;
use veoveo_mcp_contract::{
    ServerSlug,
    docs::ServerDocs,
    server_contract::{
        McpResource, McpResourceTemplate, McpServerContract, McpServerSetup, McpSetupError,
    },
};
use veoveo_media_mcp::contract::{MediaDocument, MediaResource, MediaScope};
use veoveo_media_mcp::{
    contract::{
        MediaGenerationUri, MediaPredictionIndexUri, MediaPredictionUri, MediaTaskUsageUri,
        MediaUsageIndexUri,
    },
    uris,
};
use veoveo_types::{ResourceAddress, ResourceScheme, ResourceTemplateUri};

pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("media"));
pub(super) struct MediaContract;
pub(super) static SERVER_SETUP: LazyLock<McpServerSetup<MediaContract>> =
    LazyLock::new(|| McpServerSetup::new().expect("declared Media MCP setup"));
impl McpServerContract for MediaContract {
    type Scope = MediaScope;
    type Resource = MediaResource;
    fn slug() -> ServerSlug {
        ServerSlug::new("media").expect("declared slug")
    }
    fn scheme() -> ResourceScheme {
        uris::SCHEME.clone()
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
                let address = MediaResource::parse(&descriptor.uri)
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
    let mut caps: ServerCapabilities = ServerCapabilities::builder()
        .enable_tools()
        .enable_prompts()
        .enable_resources()
        .enable_resources_subscribe()
        .enable_resources_list_changed()
        .enable_completions()
        .build();
    veoveo_mcp_apps_extension::extend_capabilities(&mut caps);
    caps.extensions.get_or_insert_default().insert(
        rmcp::model::TASKS_EXTENSION_ID.to_owned(),
        rmcp::model::JsonObject::new(),
    );
    let mut info = ServerConfig::default();
    info.capabilities = caps;
    info.server_info = rmcp::model::Implementation::new("media", env!("CARGO_PKG_VERSION"));
    info.instructions = Some(
        "Async gateway to media generation models. Workflow: \
         (1) read media://models (or use completion/complete on media://model/{+model_id}) to pick a model; \
         (2) optionally use prompts/list and prompts/get to draft model selection or media-specific briefs; \
         (3) read media://model/{+model_id} for its exact input JSON Schema; \
         (4) call the `run` tool through the negotiated task extension with {model, input}; \
         (5) read tasks/get or subscribe through subscriptions/listen; \
         (6) read its canonical media://prediction/{id}/result resource for typed Artifact metadata; \
         (7) read media://usage/task/{task_id} using the result's native Task ID for usage estimates and actual billing."
            .into(),
    );
    info
}
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
                MediaResource::Document(MediaDocument::parse(doc.id).expect("embedded document"))
                    .to_uri()
                    .expect("typed document address")
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
/// `media://contract` capability inventory.
pub(super) fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(uris::DOC_TEMPLATE, "doc")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        ResourceTemplate::new(uris::MODEL_TEMPLATE, "model")
            .with_title("Media model schema")
            .with_description(
                "Full definition of one model: input JSON Schema, pricing, description. \
                     model_id supports completion/complete.",
            )
            .with_mime_type("application/json"),
        ResourceTemplate::new(MediaPredictionUri::TEMPLATE, "prediction")
            .with_title("Media prediction state")
            .with_description(
                "Live state of a prediction. Subscribable: resources/updated fires when \
                     the provider reports a terminal state.",
            )
            .with_mime_type("application/json"),
        ResourceTemplate::new(MediaGenerationUri::TEMPLATE, "generation_result")
            .with_title("Media generation result")
            .with_description(
                "Completed generation and its published output metadata, retained with the Task.",
            )
            .with_mime_type("application/json"),
        ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
            .with_title("Media artifact")
            .with_description(
                "Server-owned immutable output artifact, addressed by occurrence id.",
            ),
        ResourceTemplate::new(MediaUsageIndexUri::TEMPLATE, "usage_page")
            .with_title("Media usage page")
            .with_mime_type("application/json"),
        ResourceTemplate::new(MediaPredictionIndexUri::TEMPLATE, "prediction_page")
            .with_title("Media prediction page")
            .with_mime_type("application/json"),
        ResourceTemplate::new(MediaTaskUsageUri::TEMPLATE, "usage")
            .with_title("Media task usage")
            .with_description("Usage estimates and actuals for one task, addressed by task id.")
            .with_mime_type("application/json"),
    ]
}

pub(super) fn resource_catalog() -> Vec<Resource> {
    let mut resources = well_known_resources();
    resources.extend([
        veoveo_mcp_apps_extension::app_resource(uris::STUDIO_APP_URI, "studio")
            .with_title("Studio")
            .with_description(
                "Generate media through governed provider models and inspect outputs.",
            ),
        Resource::new(uris::MODELS_URI, "models")
            .with_title("Media model catalog")
            .with_description(
                "Compact index of every media model: model_id, type, description, base price.",
            )
            .with_mime_type("application/json"),
        Resource::new(MediaPredictionIndexUri::ROOT, "predictions")
            .with_title("Media predictions")
            .with_description("Paged prediction identities visible to the caller.")
            .with_mime_type("application/json"),
        Resource::new(MediaUsageIndexUri::ROOT, "usage")
            .with_title("Media usage ledger")
            .with_description("Index of task usage resources.")
            .with_mime_type("application/json"),
    ]);
    resources.sort_by(|a, b| a.uri.cmp(&b.uri));
    resources
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_setup_preserves_capabilities_and_expands_all_owned_templates() {
        use std::collections::BTreeMap;
        use veoveo_media_mcp::contract::{
            MediaPredictionCursor, MediaPredictionId, MediaUsageCursor,
        };
        let setup = &*SERVER_SETUP;
        assert_eq!(setup.resources().len(), 8);
        assert_eq!(setup.resource_templates().len(), 8);
        let capabilities = &setup.server_config().capabilities;
        let resources = capabilities.resources.as_ref().unwrap();
        assert_eq!(resources.subscribe, Some(true));
        assert_eq!(resources.list_changed, Some(true));
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
            .find(|resource| resource.descriptor().uri == uris::STUDIO_APP_URI)
            .unwrap();
        let original = resource_catalog()
            .into_iter()
            .find(|resource| resource.uri == uris::STUDIO_APP_URI)
            .unwrap();
        assert_eq!(
            serde_json::to_value(app.descriptor()).unwrap(),
            serde_json::to_value(original).unwrap()
        );

        let task = veoveo_types::TaskId::new();
        let prediction = MediaPredictionId::new("provider/job ?#&=%+ 雨").unwrap();
        let variables = BTreeMap::from([
            ("doc_id".to_owned(), "design".to_owned()),
            ("task_id".to_owned(), task.to_string()),
            (
                "artifact_id".to_owned(),
                veoveo_artifact_contract::ArtifactId::new().to_string(),
            ),
            ("model_id".to_owned(), "openai/gpt-image-2/edit".to_owned()),
            ("id".to_owned(), prediction.to_string()),
        ]);
        for template in setup.resource_templates() {
            let uri = template.template().expand_scalars(&variables).unwrap();
            assert_eq!(
                MediaResource::parse(uri.as_str())
                    .unwrap_or_else(|error| panic!(
                        "template {} expanded to {uri}: {error}",
                        template.template().as_str()
                    ))
                    .to_uri()
                    .unwrap(),
                uri
            );
            if template.template().variables().any(|name| name == "cursor") {
                let mut paged = variables.clone();
                let cursor = if template.template().as_str() == MediaPredictionIndexUri::TEMPLATE {
                    MediaPredictionCursor::new(prediction.clone())
                        .unwrap()
                        .as_str()
                        .to_owned()
                } else {
                    MediaUsageCursor::new(task).unwrap().as_str().to_owned()
                };
                paged.insert("cursor".into(), cursor);
                let uri = template.template().expand_scalars(&paged).unwrap();
                assert_eq!(
                    MediaResource::parse(uri.as_str())
                        .unwrap()
                        .to_uri()
                        .unwrap(),
                    uri
                );
            }
        }
    }

    #[test]
    fn discovery_declares_static_roots_and_typed_page_templates() {
        let resources = resource_catalog();
        let uris = resources
            .iter()
            .map(|resource| resource.uri.as_str())
            .collect::<Vec<_>>();
        assert!(uris.contains(&MediaUsageIndexUri::ROOT));
        assert!(uris.contains(&MediaPredictionIndexUri::ROOT));
        assert!(uris.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            !uris.iter().any(|uri| uri.starts_with("media://usage/task/")
                || uri.starts_with("media://prediction/"))
        );
        let templates = resource_templates();
        for expected in [
            MediaUsageIndexUri::TEMPLATE,
            MediaPredictionIndexUri::TEMPLATE,
            MediaTaskUsageUri::TEMPLATE,
            MediaPredictionUri::TEMPLATE,
            MediaGenerationUri::TEMPLATE,
        ] {
            assert!(
                templates
                    .iter()
                    .any(|template| template.uri_template == expected)
            );
        }
    }
}
