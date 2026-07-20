use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock,
        ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        ReadResourceRequestParams, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerInfo, SubscribeRequestParams,
        UnsubscribeRequestParams,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Serialize;
use veoveo_mcp_contract::{GatewayInternalIdentity, Page, paginate};

use crate::{
    contract::{
        CaptureFrameRequest, CloseViewRequest, CloseViewResult, CreateViewRequest, FrameRecord,
        SetCameraRequest, ViewRecord,
    },
    server::AppState,
    uris,
};

const LIST_PAGE_SIZE: usize = 100;

#[derive(Clone)]
pub(crate) struct ViewMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<ViewMcp>,
}

#[tool_router]
impl ViewMcp {
    pub(crate) fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Create map view",
        description = "Create an owner-scoped point of view over one configured complete 3D Tiles layer. Pose, look-at, and orbit-target cameras all resolve to an exact geodetic pose.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ViewRecord>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn create_view(
        &self,
        Parameters(request): Parameters<CreateViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "view:write")?;
        let view = self
            .state
            .views
            .create_view(identity.actor.id.as_str(), request)
            .await
            .map_err(invalid_params)?;
        self.state
            .subscriptions
            .notify_resource_updated(uris::VIEWS)
            .await;
        let _ = context.peer.notify_resource_list_changed().await;
        structured_result(format!("created {}", view.view_uri), &view)
    }

    #[tool(
        title = "Set map view camera",
        description = "Replace a view camera under optimistic revision control and return its resolved exact pose.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ViewRecord>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn set_camera(
        &self,
        Parameters(request): Parameters<SetCameraRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "view:write")?;
        let view = self
            .state
            .views
            .set_camera(identity.actor.id.as_str(), request)
            .await
            .map_err(invalid_params)?;
        self.state
            .subscriptions
            .notify_resource_updated(&view.view_uri)
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::VIEWS)
            .await;
        structured_result(format!("updated {}", view.view_uri), &view)
    }

    #[tool(
        title = "Capture map view frame",
        description = "Render one hardware-accelerated offscreen image from a fixed view revision. This operation requires task-based invocation and returns image content plus typed frame metadata.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<FrameRecord>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true)
    )]
    async fn capture_frame(
        &self,
        Parameters(_request): Parameters<CaptureFrameRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "capture_frame requires task-based invocation",
            None,
        ))
    }

    #[tool(
        title = "Close map view",
        description = "Close an owner-scoped view under optimistic revision control and cancel its unfinished captures.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CloseViewResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn close_view(
        &self,
        Parameters(request): Parameters<CloseViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "view:write")?;
        let uri = uris::view(&request.view_id);
        let result = self
            .state
            .views
            .close_view(identity.actor.id.as_str(), request)
            .await
            .map_err(invalid_params)?;
        self.state.subscriptions.notify_resource_updated(uri).await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::VIEWS)
            .await;
        let _ = context.peer.notify_resource_list_changed().await;
        structured_result(format!("closed view {}", result.view_id), &result)
    }
}

#[tool_handler]
impl ServerHandler for ViewMcp {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new("view", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Create an owner-scoped view with an exact pose or target camera, replace its camera under revision control, and invoke capture_frame through the Task API. A successful capture returns a directly displayable image and view://frame metadata. The renderer has no input, picking, overlays, or feature-query behavior."
                .to_owned(),
        );
        info
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let page = mcp_page(tools, request.as_ref())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let identity = require_scope(&context, "view:read")?;
        let owner = identity.actor.id.as_str();
        let mut resources = vec![
            json_descriptor(uris::LAYERS, "View layers", "Configured 3D scene layers."),
            json_descriptor(uris::VIEWS, "Views", "Owner-scoped camera views."),
            json_descriptor(uris::FRAMES, "Frames", "Owner-scoped captured frames."),
        ];
        resources.extend(self.state.views.layers().iter().map(|layer| {
            json_descriptor(
                &uris::layer(&layer.layer_id),
                &layer.label,
                "Configured 3D scene layer without credentials.",
            )
        }));
        resources.extend(
            self.state
                .views
                .list_views(owner)
                .await
                .into_iter()
                .map(|view| json_descriptor(&view.view_uri, "View", "Camera view state.")),
        );
        resources.extend(
            self.state
                .views
                .list_frames(owner)
                .into_iter()
                .map(|frame| {
                    Resource::new(frame.frame_uri.clone(), format!("Frame {}", frame.frame_id))
                        .with_title(format!("Frame {}", frame.frame_id))
                        .with_description("Captured offscreen view image.")
                        .with_mime_type(frame.mime_type)
                }),
        );
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        let page = mcp_page(resources, request.as_ref())?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let templates = vec![
            template(
                uris::LAYER_TEMPLATE,
                "View layer",
                "Configured scene layer.",
            ),
            template(uris::VIEW_TEMPLATE, "View", "Owner-scoped camera view."),
            template(
                uris::FRAME_TEMPLATE,
                "Frame",
                "Owner-scoped captured image.",
            ),
        ];
        let page = mcp_page(templates, request.as_ref())?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(&context, "view:read")?;
        let owner = identity.actor.id.as_str();
        let uri = request.uri.as_str();
        match uri {
            uris::LAYERS => return json_resource(uri, self.state.views.layers()),
            uris::VIEWS => {
                return json_resource(uri, &self.state.views.list_views(owner).await);
            }
            uris::FRAMES => return json_resource(uri, &self.state.views.list_frames(owner)),
            _ => {}
        }
        if let Some(layer_id) = uris::parse_layer(uri) {
            let layer = self
                .state
                .views
                .layers()
                .iter()
                .find(|layer| layer.layer_id == layer_id)
                .ok_or_else(not_found)?;
            return json_resource(uri, layer);
        }
        if let Some(view_id) = uris::parse_view(uri) {
            let view = self
                .state
                .views
                .get_view(owner, &view_id)
                .await
                .map_err(|_| not_found())?;
            return json_resource(uri, &view);
        }
        if let Some(frame_id) = uris::parse_frame(uri) {
            let frame = self
                .state
                .views
                .get_frame(owner, &frame_id)
                .map_err(|_| not_found())?;
            let content = ResourceContents::blob(BASE64_STANDARD.encode(&frame.bytes), uri)
                .with_mime_type(frame.record.mime_type.clone());
            return Ok(ReadResourceResult::new(vec![content]));
        }
        Err(not_found())
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let identity = require_scope(&context, "view:read")?;
        let owner = identity.actor.id.as_str();
        let values: Vec<String> = match (reference.uri.as_str(), request.argument.name.as_str()) {
            (uris::LAYER_TEMPLATE, "layer_id") => self
                .state
                .views
                .layers()
                .iter()
                .map(|layer| layer.layer_id.to_string())
                .collect(),
            (uris::VIEW_TEMPLATE, "view_id") => self
                .state
                .views
                .list_views(owner)
                .await
                .into_iter()
                .map(|view| view.view_id.to_string())
                .collect(),
            (uris::FRAME_TEMPLATE, "frame_id") => self
                .state
                .views
                .list_frames(owner)
                .into_iter()
                .map(|frame| frame.frame_id.to_string())
                .collect(),
            _ => Vec::new(),
        };
        let needle = request.argument.value.to_ascii_lowercase();
        let matching = values
            .into_iter()
            .filter(|value| value.to_ascii_lowercase().contains(&needle))
            .collect::<Vec<_>>();
        let total = matching.len();
        let values = matching
            .into_iter()
            .take(CompletionInfo::MAX_VALUES)
            .collect();
        Ok(CompleteResult::new(
            CompletionInfo::with_pagination(
                values,
                Some(total as u32),
                total > CompletionInfo::MAX_VALUES,
            )
            .map_err(internal)?,
        ))
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = require_scope(&context, "view:read")?;
        if !is_subscribable(&request.uri) {
            return Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ));
        }
        if let Some(view_id) = uris::parse_view(&request.uri) {
            self.state
                .views
                .get_view(identity.actor.id.as_str(), &view_id)
                .await
                .map_err(|_| not_found())?;
        }
        self.state
            .subscriptions
            .subscribe(request.uri, identity.actor.id, context.peer.clone())
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = require_scope(&context, "view:read")?;
        self.state
            .subscriptions
            .unsubscribe(&request.uri, &identity.actor.id)
            .await;
        Ok(())
    }
}

pub(crate) fn frame_tool_result(
    frame: &crate::contract::CapturedFrame,
) -> anyhow::Result<CallToolResult> {
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(format!("captured {}", frame.record.frame_uri)),
        ContentBlock::image(
            BASE64_STANDARD.encode(&frame.bytes),
            frame.record.mime_type.clone(),
        ),
    ]);
    result.structured_content = Some(serde_json::to_value(&frame.record)?);
    Ok(result)
}

fn internal_identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, McpError> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<GatewayInternalIdentity>())
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))
}

fn require_scope(
    context: &RequestContext<RoleServer>,
    required: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
        .then_some(identity)
        .ok_or_else(|| McpError::invalid_request(format!("scope `{required}` is required"), None))
}

fn is_subscribable(uri: &str) -> bool {
    matches!(uri, uris::VIEWS | uris::FRAMES) || uris::parse_view(uri).is_some()
}

fn structured_result<T: Serialize>(text: String, value: &T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(value).map_err(internal)?);
    Ok(result)
}

fn json_resource<T: Serialize + ?Sized>(
    uri: &str,
    value: &T,
) -> Result<ReadResourceResult, McpError> {
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(serde_json::to_string(value).map_err(internal)?, uri)
            .with_mime_type("application/json"),
    ]))
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

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE).map_err(invalid_params)
}

fn not_found() -> McpError {
    McpError::resource_not_found("unknown View resource", None)
}

fn invalid_params(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_results_use_native_mcp_image_content() {
        let value = serde_json::to_value(ContentBlock::image("abcd", "image/png")).unwrap();
        assert_eq!(value["type"], "image");
        assert_eq!(value["mimeType"], "image/png");
    }
}
