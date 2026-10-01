use crate::contract::ViewScope;
use crate::server::setup::{SERVER_DOCS, SERVER_SETUP};
use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, CancelTaskParams,
        CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock, GetTaskParams,
        GetTaskResult, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, Reference,
        ResourceContents, ServerConfig, SubscriptionFilter, Tool, UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
};
use serde::Serialize;
use veoveo_mcp_contract::{GatewayInternalIdentity, Page, PlaneCaller, paginate};

use crate::{
    contract::{
        CaptureFrameRequest, CloseViewRequest, CloseViewResult, CreateSceneCompositionRequest,
        CreateViewRequest, FrameRecord, SceneComposition, SetCameraRequest, ViewRecord,
        ViewResource, ViewUri,
    },
    server::{AppState, auth::ForwardedBearer, tasks::ViewTaskExtension},
    source::LayerSummary,
    state::{ResourceOwner, ServiceError},
    uris,
};

const LIST_PAGE_SIZE: usize = 100;

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
        description = "Create a scene from one 3D Tiles base layer, optional inputs, an optional Frames revision, and ordered overlays. Only you can change it. Use a base_layer id listed in this tool's input schema; labels and source kinds are not ids. The same list is at view://layers.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SceneComposition>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = true)
    )]
    async fn create_scene_composition(
        &self,
        Parameters(request): Parameters<CreateSceneCompositionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, ViewScope::Write)?;
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
        structured_result(
            format!("created {}", composition.composition_uri()),
            &composition,
        )
    }

    #[tool(
        title = "Create map view",
        description = "Create a view (a camera) over one scene. Pose, look-at, and orbit-target cameras all resolve to an exact geodetic pose.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ViewRecord>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn create_view(
        &self,
        Parameters(request): Parameters<CreateViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, ViewScope::Write)?;
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
        structured_result(format!("created {}", view.view_uri()), &view)
    }

    #[tool(
        title = "Set map view camera",
        description = "Replace a view's camera and return the resolved pose. Pass the view revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ViewRecord>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn set_camera(
        &self,
        Parameters(request): Parameters<SetCameraRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, ViewScope::Write)?;
        let owner = ResourceOwner::from_identity(&identity);
        let view = self
            .state
            .views
            .set_camera(&owner, request)
            .await
            .map_err(invalid_params)?;
        self.state
            .subscriptions
            .notify_resource_updated(view.view_uri().to_string())
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::VIEWS)
            .await;
        structured_result(format!("updated {}", view.view_uri()), &view)
    }

    #[tool(
        title = "Capture map view frame",
        description = "Render one image of a view revision on the GPU and return it with typed frame metadata. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<FrameRecord>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true)
    )]
    async fn capture_frame(
        &self,
        Parameters(_request): Parameters<CaptureFrameRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`capture_frame` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Close map view",
        description = "Close a view you own and cancel its unfinished captures. Pass the view revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CloseViewResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn close_view(
        &self,
        Parameters(request): Parameters<CloseViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, ViewScope::Write)?;
        let owner = ResourceOwner::from_identity(&identity);
        let uri = ViewUri::new(request.view_id.clone());
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

    fn get_info(&self) -> ServerConfig {
        SERVER_SETUP.server_config().clone()
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
        let identity = require_scope(&context, ViewScope::Read)?;
        let resources = crate::server::setup::visible_resources(&identity.actor.scopes);
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
        let page = mcp_page(
            SERVER_SETUP
                .resource_templates()
                .iter()
                .map(|template| template.descriptor().clone())
                .collect(),
            request.as_ref(),
        )?;
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
        if let Some(result) = SERVER_SETUP.read_documents(&request, &context)? {
            return Ok(result);
        }
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        async {
            let uri = request.uri.as_str();
            let address = ViewResource::parse(uri).map_err(|_| not_found())?;
            if address == ViewResource::PreviewApp {
                require_scope(&context, ViewScope::Capture)?;
                return Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(
                        uri,
                        crate::app::preview_app_html(),
                    ),
                ]));
            }
            let identity = require_scope(&context, ViewScope::Read)?;
            let owner = ResourceOwner::from_identity(&identity);
            match address {
                ViewResource::Docs => json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>()),
                ViewResource::Document(id) => {
                    let doc = SERVER_DOCS.doc(id.as_str()).ok_or_else(not_found)?;
                    Ok(ReadResourceResult::new(vec![
                        ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                    ]))
                }
                ViewResource::Contract => json_resource(uri, SERVER_DOCS.contract_declaration()),
                ViewResource::Layers => json_resource(uri, self.state.views.layers()),
                ViewResource::Compositions => {
                    json_resource(uri, &self.state.views.list_scene_compositions(&owner).await)
                }
                ViewResource::Views => {
                    json_resource(uri, &self.state.views.list_views(&owner).await)
                }
                ViewResource::Frames => json_resource(uri, &self.state.views.list_frames(&owner)),
                ViewResource::Layer(address) => {
                    let layer = self
                        .state
                        .views
                        .layers()
                        .iter()
                        .find(|layer| &layer.layer_id == address.id())
                        .ok_or_else(not_found)?;
                    json_resource(uri, layer)
                }
                ViewResource::Scene(address) => {
                    let record = self
                        .state
                        .views
                        .preview_scene(
                            &owner,
                            address.view_id(),
                            address.policy(),
                            context.ct.child_token(),
                        )
                        .await
                        .map_err(read_error)?;
                    json_resource(uri, &record)
                }
                ViewResource::Tile(address) => {
                    let (bytes, mime) = self
                        .state
                        .views
                        .read_tile_bytes(address.id(), context.ct.child_token())
                        .await
                        .map_err(read_error)?;
                    Ok(ReadResourceResult::new(vec![
                        ResourceContents::blob(BASE64_STANDARD.encode(bytes.as_slice()), uri)
                            .with_mime_type(mime),
                    ]))
                }
                ViewResource::View(address) => {
                    let view = self
                        .state
                        .views
                        .get_view(&owner, address.id())
                        .await
                        .map_err(|_| not_found())?;
                    json_resource(uri, &view)
                }
                ViewResource::Composition(address) => {
                    let composition = self
                        .state
                        .views
                        .get_scene_composition(&owner, address.id())
                        .await
                        .map_err(|_| not_found())?;
                    json_resource(uri, &composition)
                }
                ViewResource::Frame(address) => {
                    let frame = self
                        .state
                        .views
                        .get_frame(&owner, address.id())
                        .map_err(|_| not_found())?;
                    Ok(ReadResourceResult::new(vec![
                        ResourceContents::blob(BASE64_STANDARD.encode(frame.bytes()), uri)
                            .with_mime_type(frame.record().mime_type()),
                    ]))
                }
                ViewResource::PreviewApp => unreachable!("App handled under capture permission"),
            }
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
        let identity = require_scope(&context, ViewScope::Read)?;
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
                .map(|view| view.view_id().to_string())
                .collect(),
            (uris::COMPOSITION_TEMPLATE, "composition_id") => self
                .state
                .views
                .list_scene_compositions(&owner)
                .await
                .into_iter()
                .map(|composition| composition.composition_id().to_string())
                .collect(),
            (uris::FRAME_TEMPLATE, "frame_id") => self
                .state
                .views
                .list_frames(&owner)
                .into_iter()
                .map(|frame| frame.frame_id().to_string())
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
        crate::server::setup::accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let request_context = context.request_context().clone();
        let identity = require_scope(&request_context, ViewScope::Read)?;
        let owner = ResourceOwner::from_identity(&identity);
        for uri in context.accepted().resource_subscriptions.iter().flatten() {
            match ViewResource::parse(uri).map_err(|_| not_found())? {
                ViewResource::Compositions | ViewResource::Views | ViewResource::Frames => {}
                ViewResource::View(address) => {
                    self.state
                        .views
                        .get_view(&owner, address.id())
                        .await
                        .map_err(|_| not_found())?;
                }
                _ => {
                    return Err(McpError::invalid_params(
                        "resource is immutable or not subscribable",
                        None,
                    ));
                }
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
        ContentBlock::text(format!("captured {}", frame.record().frame_uri())),
        ContentBlock::image(
            BASE64_STANDARD.encode(frame.bytes()),
            frame.record().mime_type(),
        ),
    ]);
    result.structured_content = Some(serde_json::to_value(frame.record())?);
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
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })
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
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })?;
    let memberships = identity.actor.group_memberships();
    Ok(PlaneCaller {
        bearer_token,
        identity,
        memberships,
    })
}

fn require_scope(
    context: &RequestContext<RoleServer>,
    required: ViewScope,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    crate::server::auth::require_scope(&identity, required)?;
    Ok(identity)
}

fn read_error(error: crate::state::ServiceError) -> McpError {
    match error {
        crate::state::ServiceError::ViewNotFound | crate::state::ServiceError::TileNotFound => {
            not_found()
        }
        other => McpError::invalid_request(other.to_string(), None),
    }
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
