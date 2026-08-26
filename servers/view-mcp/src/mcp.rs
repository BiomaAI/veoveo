use std::sync::{Arc, LazyLock};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, CancelTaskParams,
        CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock, GetTaskParams,
        GetTaskResult, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, Reference, Resource,
        ResourceContents, ResourceTemplate, ServerCapabilities, ServerInfo, SubscriptionFilter,
        Tool, UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
};
use serde::Serialize;
use veoveo_mcp_contract::{GatewayInternalIdentity, Page, PlaneCaller, docs::ServerDocs, paginate};

use crate::{
    contract::{
        CaptureFrameRequest, CloseViewRequest, CloseViewResult, CreateSceneCompositionRequest,
        CreateViewRequest, FrameRecord, SceneComposition, SetCameraRequest, ViewRecord,
    },
    server::{AppState, auth::ForwardedBearer, tasks::ViewTaskExtension},
    source::LayerSummary,
    state::{ResourceOwner, ServiceError},
    uris,
};

const LIST_PAGE_SIZE: usize = 100;

/// The crate documents embedded at build time and served under the well-known
/// surface: `view://docs`, `view://docs/{doc_id}`, `view://contract`, and the
/// administrative `admin/docs` routes (contract C18-C21).
pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!(crate::server::SERVER_SLUG));

/// The real view lifecycle tools double as the preview app's surface; the
/// app drives them end-to-end (revision control and task-based capture
/// included) rather than any parallel convenience tools.
const PREVIEW_APP_TOOLS: &[&str] = &[
    "create_scene_composition",
    "create_view",
    "set_camera",
    "capture_frame",
    "close_view",
];

const PREVIEW_APP_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNCIgaGVpZ2h0PSIyNCIgdmlld0JveD0iMCAwIDI0IDI0IiBmaWxsPSJub25lIiBzdHJva2U9IiM0YTdkZDYiIHN0cm9rZS13aWR0aD0iMiIgc3Ryb2tlLWxpbmVjYXA9InJvdW5kIiBzdHJva2UtbGluZWpvaW49InJvdW5kIj48cGF0aCBkPSJtMTQgMTAgNy0zdjEwbC03LTMiLz48cmVjdCB4PSIyIiB5PSI3IiB3aWR0aD0iMTIiIGhlaWdodD0iMTAiIHJ4PSIyIi8+PC9zdmc+";

#[derive(Clone)]
pub(crate) struct ViewMcp {
    state: Arc<AppState>,
    task_service: ViewTaskExtension,
    #[allow(dead_code)]
    tool_router: ToolRouter<ViewMcp>,
}

#[tool_router]
impl ViewMcp {
    pub(crate) fn new(state: Arc<AppState>) -> Self {
        Self {
            task_service: ViewTaskExtension::new(state.clone()),
            state,
            tool_router: Self::tool_router(),
        }
    }

    /// The capability inventory declared at `view://contract` (contract C19).
    ///
    #[tool(
        title = "Create scene composition",
        description = "Create an immutable owner-scoped scene composition from one configured 3D Tiles base layer, exact governed inputs, an optional Frames revision binding, and bounded ordered overlays. Use an exact base_layer identifier advertised by this tool's runtime input schema; labels and source kinds are not identifiers. The credential-free catalog is also available at view://layers.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SceneComposition>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = true)
    )]
    async fn create_scene_composition(
        &self,
        Parameters(request): Parameters<CreateSceneCompositionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "view:write")?;
        let caller = plane_caller(&context, identity.clone())?;
        let composition = self
            .state
            .views
            .create_scene_composition(&identity, &caller, request)
            .await
            .map_err(|error| invalid_scene_composition_params(error, self.state.views.layers()))?;
        self.state
            .subscriptions
            .notify_resource_updated(uris::COMPOSITIONS)
            .await;
        self.state
            .subscriptions
            .notify_resource_list_changed()
            .await;
        structured_result(
            format!("created {}", composition.composition_uri),
            &composition,
        )
    }

    #[tool(
        title = "Create map view",
        description = "Create an owner-scoped point of view over one immutable scene composition. Pose, look-at, and orbit-target cameras all resolve to an exact geodetic pose.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ViewRecord>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn create_view(
        &self,
        Parameters(request): Parameters<CreateViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "view:write")?;
        let owner = ResourceOwner::from_identity(&identity);
        let view = self
            .state
            .views
            .create_view(&owner, request)
            .await
            .map_err(invalid_params)?;
        self.state
            .subscriptions
            .notify_resource_updated(uris::VIEWS)
            .await;
        self.state
            .subscriptions
            .notify_resource_list_changed()
            .await;
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
        let owner = ResourceOwner::from_identity(&identity);
        let view = self
            .state
            .views
            .set_camera(&owner, request)
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
        let owner = ResourceOwner::from_identity(&identity);
        let uri = uris::view(&request.view_id);
        let result = self
            .state
            .views
            .close_view(&owner, request)
            .await
            .map_err(invalid_params)?;
        self.state.subscriptions.notify_resource_updated(uri).await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::VIEWS)
            .await;
        self.state
            .subscriptions
            .notify_resource_list_changed()
            .await;
        structured_result(format!("closed view {}", result.view_id), &result)
    }
}

#[tool_handler]
impl ServerHandler for ViewMcp {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }

    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
        capabilities.extensions.get_or_insert_default().insert(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            rmcp::model::JsonObject::new(),
        );
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new("view", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Create an immutable scene composition from a configured 3D Tiles base layer and exact governed overlay inputs, then create an owner-scoped view with an exact pose or target camera. Replace its camera under revision control and invoke capture_frame through the Task API with an explicit scene time. A successful capture returns a directly displayable hardware-rendered image plus view://frame provenance. The ui://view/preview.html app drives the same lifecycle interactively; its parameterized view-scene manifests reference view://tile/... draco GLB resources selected for the requested viewport and screen-space error."
                .to_owned(),
        );
        info
    }

    async fn call_tool(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if let Some(created) =
            veoveo_task_runtime::start_durable_tool_task(&self.task_service, &mut request, &context)
                .await?
        {
            return Ok(created.into());
        }
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(call).await
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::get_task(&self.task_service, &caller, request)
            .await
    }

    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::update_task(&self.task_service, &caller, request)
            .await
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::cancel_task(
            &self.task_service,
            &caller,
            request.task_id,
        )
        .await
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        // The #[tool] macro has no meta attribute; app links attach here.
        tools = tools
            .into_iter()
            .map(|tool| advertise_configured_layers(tool, self.state.views.layers()))
            .map(|tool| {
                if PREVIEW_APP_TOOLS.contains(&tool.name.as_ref()) {
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        tool,
                        uris::PREVIEW_APP_URI,
                        &[
                            veoveo_mcp_apps_extension::UiVisibility::Model,
                            veoveo_mcp_apps_extension::UiVisibility::App,
                        ],
                    )
                } else {
                    tool
                }
            })
            .collect();
        let page = mcp_page(tools, request.as_ref())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let identity = require_scope(&context, "view:read")?;
        let owner = ResourceOwner::from_identity(&identity);
        let mut resources = well_known_resources();
        resources.extend([
            json_descriptor(uris::LAYERS, "View layers", "Configured 3D scene layers."),
            json_descriptor(
                uris::COMPOSITIONS,
                "Scene compositions",
                "Owner-scoped immutable governed scene compositions.",
            ),
            json_descriptor(uris::VIEWS, "Views", "Owner-scoped camera views."),
            json_descriptor(uris::FRAMES, "Frames", "Owner-scoped captured frames."),
        ]);
        if identity_has_scope(&identity, "view:capture") {
            resources.push(
                veoveo_mcp_apps_extension::app_resource(uris::PREVIEW_APP_URI, "view-preview-app")
                    .with_title("Preview")
                    .with_description(
                        "Interactive MCP App that composes camera poses over configured 3D \
                         Tiles layers, previews the scene in-browser, and drives the real \
                         view lifecycle including task-based capture.",
                    )
                    .with_icons(vec![rmcp::model::Icon::new(PREVIEW_APP_ICON)]),
            );
        }
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
                .list_scene_compositions(&owner)
                .await
                .into_iter()
                .map(|composition| {
                    json_descriptor(
                        &composition.composition_uri,
                        "Scene composition",
                        "Immutable governed scene composition.",
                    )
                }),
        );
        resources.extend(
            self.state
                .views
                .list_views(&owner)
                .await
                .into_iter()
                .map(|view| json_descriptor(&view.view_uri, "View", "Camera view state.")),
        );
        resources.extend(
            self.state
                .views
                .list_frames(&owner)
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
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let page = mcp_page(resource_templates(), request.as_ref())?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        async {
            let uri = request.uri.as_str();
            // The app is gated like the tools it drives, ahead of the blanket
            // read gate: view:capture holders may lack nothing the app needs.
            if uri == uris::PREVIEW_APP_URI {
                require_scope(&context, "view:capture")?;
                return Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(
                        uri,
                        crate::app::preview_app_html(),
                    ),
                ]));
            }
            let identity = require_scope(&context, "view:read")?;
            // Well-known surface (contract C18, C19): readable by any identity
            // that can list resources.
            if uri == uris::DOCS {
                return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
            }
            if let Some(doc_id) = uris::parse_doc(uri) {
                let doc = SERVER_DOCS.doc(doc_id).ok_or_else(not_found)?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                ]));
            }
            if uri == uris::CONTRACT {
                return json_resource(uri, SERVER_DOCS.contract_declaration());
            }
            let owner = ResourceOwner::from_identity(&identity);
            match uri {
                uris::LAYERS => return json_resource(uri, self.state.views.layers()),
                uris::COMPOSITIONS => {
                    return json_resource(
                        uri,
                        &self.state.views.list_scene_compositions(&owner).await,
                    );
                }
                uris::VIEWS => {
                    return json_resource(uri, &self.state.views.list_views(&owner).await);
                }
                uris::FRAMES => return json_resource(uri, &self.state.views.list_frames(&owner)),
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
            if let Some((view_id, policy)) =
                uris::parse_view_scene(uri).map_err(|error| read_error(error.into()))?
            {
                let record = self
                    .state
                    .views
                    .preview_scene(&owner, &view_id, policy, context.ct.child_token())
                    .await
                    .map_err(read_error)?;
                return json_resource(uri, &record);
            }
            if let Some(tile_key) = uris::parse_tile(uri) {
                let (bytes, mime) = self
                    .state
                    .views
                    .read_tile_bytes(&tile_key, context.ct.child_token())
                    .await
                    .map_err(read_error)?;
                let content = ResourceContents::blob(BASE64_STANDARD.encode(bytes.as_slice()), uri)
                    .with_mime_type(mime);
                return Ok(ReadResourceResult::new(vec![content]));
            }
            if let Some(view_id) = uris::parse_view(uri) {
                let view = self
                    .state
                    .views
                    .get_view(&owner, &view_id)
                    .await
                    .map_err(|_| not_found())?;
                return json_resource(uri, &view);
            }
            if let Some(composition_id) = uris::parse_composition(uri) {
                let composition = self
                    .state
                    .views
                    .get_scene_composition(&owner, &composition_id)
                    .await
                    .map_err(|_| not_found())?;
                return json_resource(uri, &composition);
            }
            if let Some(frame_id) = uris::parse_frame(uri) {
                let frame = self
                    .state
                    .views
                    .get_frame(&owner, &frame_id)
                    .map_err(|_| not_found())?;
                let content = ResourceContents::blob(BASE64_STANDARD.encode(&frame.bytes), uri)
                    .with_mime_type(frame.record.mime_type.clone());
                return Ok(ReadResourceResult::new(vec![content]));
            }
            Err(not_found())
        }
        .await
        .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
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
        let owner = ResourceOwner::from_identity(&identity);
        let values: Vec<String> = match (reference.uri.as_str(), request.argument.name.as_str()) {
            (uris::DOC_TEMPLATE, "doc_id") => {
                SERVER_DOCS.iter().map(|doc| doc.id.to_owned()).collect()
            }
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
                .list_views(&owner)
                .await
                .into_iter()
                .map(|view| view.view_id.to_string())
                .collect(),
            (uris::COMPOSITION_TEMPLATE, "composition_id") => self
                .state
                .views
                .list_scene_compositions(&owner)
                .await
                .into_iter()
                .map(|composition| composition.composition_id.to_string())
                .collect(),
            (uris::FRAME_TEMPLATE, "frame_id") => self
                .state
                .views
                .list_frames(&owner)
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

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let request_context = context.request_context().clone();
        let identity = require_scope(&request_context, "view:read")?;
        let owner = ResourceOwner::from_identity(&identity);
        for uri in context.accepted().resource_subscriptions.iter().flatten() {
            if !is_subscribable(uri) {
                return Err(McpError::invalid_params(
                    "resource is immutable or not subscribable",
                    None,
                ));
            }
            if let Some(view_id) = uris::parse_view(uri) {
                self.state
                    .views
                    .get_view(&owner, &view_id)
                    .await
                    .map_err(|_| not_found())?;
            }
        }
        veoveo_task_runtime::listen_durable_subscriptions(
            &self.task_service,
            context,
            Some(&self.state.subscriptions),
            None,
        )
        .await
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

fn plane_caller(
    context: &RequestContext<RoleServer>,
    identity: GatewayInternalIdentity,
) -> Result<PlaneCaller, McpError> {
    let bearer_token = context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<ForwardedBearer>())
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    let memberships = identity.actor.group_memberships();
    Ok(PlaneCaller {
        bearer_token,
        identity,
        memberships,
    })
}

fn require_scope(
    context: &RequestContext<RoleServer>,
    required: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    identity_has_scope(&identity, required)
        .then_some(identity)
        .ok_or_else(|| McpError::invalid_request(format!("scope `{required}` is required"), None))
}

fn identity_has_scope(identity: &GatewayInternalIdentity, required: &str) -> bool {
    identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
}

fn read_error(error: crate::state::ServiceError) -> McpError {
    match error {
        crate::state::ServiceError::ViewNotFound | crate::state::ServiceError::TileNotFound => {
            not_found()
        }
        other => McpError::invalid_request(other.to_string(), None),
    }
}

fn is_subscribable(uri: &str) -> bool {
    matches!(uri, uris::COMPOSITIONS | uris::VIEWS | uris::FRAMES)
        || uris::parse_view(uri).is_some()
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

/// Well-known surface resources (contract C18, C19). `list_resources` serves
/// these for every authorized identity and `stable_resource_uris` declares
/// them in the `view://contract` capability inventory, so the two cannot
/// diverge.
fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![json_descriptor(
        uris::DOCS,
        "Server documents",
        "Index of the crate documents embedded at build time.",
    )];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(uris::doc(doc.id), doc.title)
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
        template(
            uris::FRAME_TEMPLATE,
            "Frame",
            "Owner-scoped captured image.",
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

fn invalid_scene_composition_params(error: ServiceError, layers: &[LayerSummary]) -> McpError {
    let ServiceError::LayerNotFound(requested) = error else {
        return invalid_params(error);
    };
    let valid = layers
        .iter()
        .map(|layer| format!("`{}`", layer.layer_id))
        .collect::<Vec<_>>()
        .join(", ");
    invalid_params(format!(
        "scene layer `{requested}` is not configured; valid base_layer identifiers: [{valid}]; read view://layers for labels and source kinds"
    ))
}

fn advertise_configured_layers(mut tool: Tool, layers: &[LayerSummary]) -> Tool {
    if tool.name.as_ref() != "create_scene_composition" {
        return tool;
    }
    let identifiers = layers
        .iter()
        .map(|layer| serde_json::Value::String(layer.layer_id.to_string()))
        .collect::<Vec<_>>();
    let schema = Arc::make_mut(&mut tool.input_schema);
    let Some(base_layer) = schema
        .get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
        .and_then(|properties| properties.get_mut("base_layer"))
        .and_then(serde_json::Value::as_object_mut)
    else {
        tracing::error!("generated create_scene_composition schema omitted base_layer");
        return tool;
    };
    base_layer.insert(
        "description".to_owned(),
        serde_json::Value::String(
            "Exact configured layer_id. Use one of the runtime-advertised enum values; do not use the display label or source_kind. Read view://layers for the credential-free catalog."
                .to_owned(),
        ),
    );
    base_layer.insert(
        "enum".to_owned(),
        serde_json::Value::Array(identifiers.clone()),
    );
    if let [identifier] = identifiers.as_slice() {
        base_layer.insert("default".to_owned(), identifier.clone());
    }
    tool
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

#[cfg(test)]
mod tests {
    use crate::{contract::LayerId, source::LayerSummary, state::ServiceError};

    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!ViewMcp::tool_router().list_all().is_empty());
    }

    #[test]
    fn scene_composition_schema_advertises_runtime_layer_identifiers() {
        let tool = ViewMcp::tool_router()
            .list_all()
            .into_iter()
            .find(|tool| tool.name.as_ref() == "create_scene_composition")
            .expect("composition tool");
        let layer = LayerSummary {
            layer_id: LayerId::new("google-photorealistic").unwrap(),
            label: "Google Photorealistic 3D Tiles".to_owned(),
            source_kind: "google_photorealistic".to_owned(),
        };
        let tool = advertise_configured_layers(tool, &[layer]);
        let schema = serde_json::Value::Object(tool.input_schema.as_ref().clone());
        let base_layer = schema
            .pointer("/properties/base_layer")
            .expect("base_layer schema");
        assert_eq!(
            base_layer["enum"],
            serde_json::json!(["google-photorealistic"])
        );
        assert_eq!(base_layer["default"], "google-photorealistic");
        assert!(
            base_layer["description"]
                .as_str()
                .is_some_and(|description| description.contains("view://layers"))
        );
    }

    #[test]
    fn unknown_layer_error_returns_exact_recovery_values() {
        let layers = [LayerSummary {
            layer_id: LayerId::new("google-photorealistic").unwrap(),
            label: "Google Photorealistic 3D Tiles".to_owned(),
            source_kind: "google_photorealistic".to_owned(),
        }];
        let error = invalid_scene_composition_params(
            ServiceError::LayerNotFound(LayerId::new("google").unwrap()),
            &layers,
        );
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(error.message.contains("`google-photorealistic`"));
        assert!(error.message.contains("view://layers"));
    }

    #[test]
    fn frame_results_use_native_mcp_image_content() {
        let value = serde_json::to_value(ContentBlock::image("abcd", "image/png")).unwrap();
        assert_eq!(value["type"], "image");
        assert_eq!(value["mimeType"], "image/png");
    }
}

#[cfg(test)]
mod well_known_tests {
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::SERVER_DOCS;

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "view");
        let agents = SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
        assert!(agents.body.contains("## Contract Compliance"));
        let design = SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
        assert!(!design.body.is_empty());
        let index = SERVER_DOCS.llms_txt();
        assert!(index.contains("(agents)"));
        assert!(index.contains("(design)"));
    }

    #[test]
    fn contract_declaration_resolves_from_the_embedded_manual() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        assert_eq!(declaration.server, "view");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        let json = serde_json::to_value(&declaration).expect("declaration serializes");
        assert_eq!(json["server"], "view");
    }

    #[test]
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
