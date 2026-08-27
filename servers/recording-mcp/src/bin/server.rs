use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use clap::Parser;
use re_protos::cloud::v1alpha1::rerun_cloud_service_server::RerunCloudServiceServer;
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock,
        GetPromptRequestParams, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt,
        ReadResourceRequestParams, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerInfo, SubscriptionFilter,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
    transport::streamable_http_server::StreamableHttpService,
};
use secrecy::ExposeSecret as _;
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    CreateRecordingProjectionRequest, GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier,
    GatewayInternalTrustBundle, Page, RecordingProjectionHandle, ServerSlug, SubscriptionHub,
    TelemetryGuard, TokenIssuer, init_server_telemetry, paginate,
};
use veoveo_platform_store::{
    PlatformStore, RecordingId, RecordingProjectionReceiptId, StoreConfig, StoreCredentials,
};
use veoveo_recording_hub::{GatewayLayerPublisher, GatewayLayerPublisherConfig};
use veoveo_recording_mcp::blueprint_playback::recording_scoped_blueprint;
use veoveo_recording_mcp::live_stream::{
    FRAMED_RRD_CONTENT_TYPE, LIVE_RRD_START_HEADER, LiveRrdStart, authorized_live_rrd_stream,
};
use veoveo_recording_mcp::{
    RecordingService,
    admin::{self, SERVER_DOCS},
    contract::{CreateCatalogGrantRequest, SealRecordingOutput, SealRecordingRequest},
    layer_cache::LayerCacheLimits,
    playback::{
        PlaybackManager, RECORDING_GRANT_HEADER, playback_application_id, playback_store_id,
    },
    service::ProjectionRuntimeLimits,
    uris,
};

#[path = "server/auth.rs"]
mod auth;
#[path = "server/config.rs"]
mod config;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/state.rs"]
mod state;

use auth::{
    InternalAuthState, artifact_caller, artifact_caller_from_context, authenticate, identity,
};
use config::Args;
use prompts::RecordingPrompt;
use state::AppState;

const SERVER_SLUG: &str = "recording";
const LIST_PAGE_SIZE: usize = 100;
const EXPLORER_TOOLS: &[&str] = &["create_recording_projection", "seal_recording"];

#[derive(Clone)]
struct RecordingMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<RecordingMcp>,
}

#[tool_router]
impl RecordingMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Seal recording",
        description = "Validate committed immutable layers, publish the v9 manifest Artifact, then atomically seal the recording. Requires admin:manage scope.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SealRecordingOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn seal_recording(
        &self,
        Parameters(request): Parameters<SealRecordingRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let recording_id = parse_recording_id(&request.recording_id)?;
        let identity = identity(&context)?;
        let output = self
            .state
            .recordings
            .seal(&identity, recording_id)
            .await
            .map_err(invalid_params)?;
        self.state
            .subscribers
            .notify_resource_updated(uris::recording_uri(&request.recording_id))
            .await;
        self.state
            .subscribers
            .notify_resource_updated(uris::layers_uri(&request.recording_id))
            .await;
        self.state.subscribers.notify_resource_list_changed().await;
        structured_result("recording sealed".to_owned(), &output)
    }

    #[tool(
        title = "Create recording projection",
        description = "Materialize one deterministic, bounded Apache Arrow stream from exact entities and components in one governed immutable recording. The returned handle contains no bearer credentials.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RecordingProjectionHandle>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn create_recording_projection(
        &self,
        Parameters(request): Parameters<CreateRecordingProjectionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = identity(&context)?;
        let artifact_caller = artifact_caller_from_context(&context, identity.clone())?;
        let cancellation = context.ct.clone();
        match self
            .state
            .recordings
            .create_projection(&identity, &artifact_caller, request, cancellation)
            .await
        {
            Ok(handle) => structured_result("recording projection ready".to_owned(), &handle),
            Err(error) => {
                tracing::warn!(%error, "recording projection request failed");
                Err(McpError::invalid_params(
                    "recording projection request was rejected",
                    None,
                ))
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for RecordingMcp {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }

    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new(SERVER_SLUG, env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Governed access to the installation recording catalog. Discover recordings through resources, materialize deterministic bounded Apache Arrow projections from committed immutable layers with create_recording_projection, and seal only frozen recordings when the caller has admin:manage scope. Sealing returns artifact:// occurrence URIs; artifact policy controls subsequent reads and sharing."
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
        tools = tools
            .into_iter()
            .map(|tool| {
                if EXPLORER_TOOLS.contains(&tool.name.as_ref()) {
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        tool,
                        uris::EXPLORER_APP_URI,
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
        let identity = identity(&context)?;
        let mut resources = vec![
            veoveo_mcp_apps_extension::app_resource(uris::EXPLORER_APP_URI, "explorer")
                .with_title("Explorer")
                .with_description(
                    "Governed recording catalog, lifecycle, and bounded timeline queries.",
                ),
            Resource::new(uris::DOCS_URI, "recording docs")
                .with_title("Server documents")
                .with_description("Index of the crate documents embedded at build time.")
                .with_mime_type("application/json"),
            Resource::new(uris::CONTRACT_URI, "recording contract")
                .with_title("Contract declaration")
                .with_description(
                    "Machine-readable contract revision, compliance, and capability inventory.",
                )
                .with_mime_type("application/json"),
            Resource::new(uris::CATALOG_URI, "recording catalog")
                .with_title("Recording catalog")
                .with_description("Authorized recording lifecycle and artifact index.")
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
        for recording in self
            .state
            .recordings
            .list_visible(&identity)
            .await
            .map_err(internal)?
        {
            resources.push(
                Resource::new(
                    uris::recording_uri(&recording.recording_id),
                    format!("recording {}", recording.recording_key),
                )
                .with_title(format!("Recording {}", recording.recording_key))
                .with_description("Governed recording metadata and seal state.")
                .with_mime_type("application/json"),
            );
            resources.push(
                Resource::new(
                    uris::layers_uri(&recording.recording_id),
                    format!("layers for {}", recording.recording_key),
                )
                .with_title(format!("Layers for {}", recording.recording_key))
                .with_description("Immutable layer publication and Artifact state.")
                .with_mime_type("application/json"),
            );
        }
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
        let templates = vec![
            ResourceTemplate::new(uris::DOC_TEMPLATE, "doc")
                .with_title("Server document")
                .with_description("Embedded crate document body (contract C18).")
                .with_mime_type("text/markdown"),
            ResourceTemplate::new(uris::RECORDING_TEMPLATE, "recording")
                .with_title("Recording")
                .with_description("Governed recording metadata by UUIDv7.")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::LAYERS_TEMPLATE, "recording layers")
                .with_title("Recording layers")
                .with_description("Durable layers for one governed recording.")
                .with_mime_type("application/json"),
        ];
        let page = mcp_page(templates, request.as_ref())?;
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
            let identity = identity(&context)?;
            let uri = request.uri.as_str();
            // Well-known surface (contract C18, C19): readable by any identity
            // that can list resources.
            if uri == uris::DOCS_URI {
                return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
            }
            if let Some(doc_id) = uris::parse_doc(uri) {
                let doc = SERVER_DOCS.doc(doc_id).ok_or_else(|| {
                    McpError::resource_not_found("server document not found", None)
                })?;
                return Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                ]));
            }
            if uri == uris::CONTRACT_URI {
                return json_resource(uri, SERVER_DOCS.contract_declaration());
            }
            if uri == uris::EXPLORER_APP_URI {
                let html = veoveo_mcp_apps_extension::workbench_app_html(
                    &veoveo_mcp_apps_extension::WorkbenchApp {
                        app_id: "recording-explorer",
                        title: "Explorer",
                        subtitle: "Browse governed recordings and inspect bounded timeline data",
                        empty_message: "No recordings are visible to this identity.",
                        resources: &[veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Recording catalog",
                            uri: uris::CATALOG_URI,
                        }],
                        tools: &[
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Create bounded Arrow projection",
                                name: "create_recording_projection",
                                arguments_json: r#"{"dataset_id":"","recording_id":"","entity_paths":["/sensor"],"component_ids":["Scalars:scalars"],"timeline":"tick","sampling":{"kind":"range","start":0,"end":100},"sparse_fill":"none","maximum_entities":8,"maximum_columns":8,"maximum_samples":1000,"maximum_rows":10000,"maximum_bytes":33554432,"deadline_ms":15000,"idempotency_key":"","units":{},"coordinate_frame_refs":[]}"#,
                            },
                            veoveo_mcp_apps_extension::WorkbenchTool {
                                label: "Seal recording",
                                name: "seal_recording",
                                arguments_json: r#"{"recording_id":""}"#,
                            },
                        ],
                        stream_result: Some(
                            veoveo_mcp_apps_extension::WorkbenchStreamResult::RecordingProjection {
                                tool_name: "create_recording_projection",
                            },
                        ),
                    },
                );
                return Ok(ReadResourceResult::new(vec![
                    veoveo_mcp_apps_extension::app_html_contents(uri, &html),
                ]));
            }
            if uri == uris::CATALOG_URI {
                return json_resource(
                    uri,
                    &self
                        .state
                        .recordings
                        .list_visible(&identity)
                        .await
                        .map_err(internal)?,
                );
            }
            if let Some(value) = uris::parse_layers_uri(uri) {
                let recording_id = parse_recording_id(value)?;
                let layers = self
                    .state
                    .recordings
                    .layer_views(&identity, recording_id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| McpError::resource_not_found("recording not found", None))?;
                return json_resource(uri, &layers);
            }
            if let Some(value) = uris::parse_recording_uri(uri) {
                let recording_id = parse_recording_id(value)?;
                let recording = self
                    .state
                    .recordings
                    .recording_view(&identity, recording_id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| McpError::resource_not_found("recording not found", None))?;
                return json_resource(uri, &recording);
            }
            Err(McpError::resource_not_found(
                format!("unknown recording resource `{uri}`"),
                None,
            ))
        }
        .await
        .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts: Vec<Prompt> = RecordingPrompt::ALL
            .into_iter()
            .map(RecordingPrompt::definition)
            .collect();
        let page = mcp_page(prompts, request.as_ref())?;
        Ok(ListPromptsResult {
            prompts: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::GetPromptResponse, McpError> {
        async {
            RecordingPrompt::by_name(&request.name)
                .ok_or_else(|| McpError::invalid_params("unknown recording prompt", None))?
                .render(request.arguments)
        }
        .await
        .map(Into::into)
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let request_context = context.request_context().clone();
        let identity = identity(&request_context)?;
        for uri in context.accepted().resource_subscriptions.iter().flatten() {
            let recording_id = subscribable_recording_id(uri)?;
            if self
                .state
                .recordings
                .recording_view(&identity, recording_id)
                .await
                .map_err(internal)?
                .is_none()
            {
                return Err(McpError::resource_not_found("recording not found", None));
            }
        }
        veoveo_mcp_contract::listen_resources(context, &self.state.subscribers, None).await
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        if !matches!(
            reference.uri.as_str(),
            uris::RECORDING_TEMPLATE | uris::LAYERS_TEMPLATE
        ) || request.argument.name != "recording_id"
        {
            return Ok(CompleteResult::default());
        }
        let identity = identity(&context)?;
        let needle = request.argument.value.to_lowercase();
        let all = self
            .state
            .recordings
            .list_visible(&identity)
            .await
            .map_err(internal)?;
        let total_matches = all
            .iter()
            .filter(|recording| {
                recording.recording_id.to_lowercase().contains(&needle)
                    || recording.recording_key.to_lowercase().contains(&needle)
            })
            .count();
        let values = all
            .into_iter()
            .filter(|recording| {
                recording.recording_id.to_lowercase().contains(&needle)
                    || recording.recording_key.to_lowercase().contains(&needle)
            })
            .map(|recording| recording.recording_id)
            .take(CompletionInfo::MAX_VALUES)
            .collect::<Vec<_>>();
        let completion = CompletionInfo::with_pagination(
            values,
            Some(total_matches as u32),
            total_matches > CompletionInfo::MAX_VALUES,
        )
        .map_err(internal)?;
        Ok(CompleteResult::new(completion))
    }
}

fn structured_result<T: Serialize>(text: String, value: &T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(value).map_err(internal)?);
    Ok(result)
}

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, McpError> {
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

fn parse_recording_id(value: &str) -> Result<RecordingId, McpError> {
    let id = uuid::Uuid::parse_str(value)
        .map_err(|_| McpError::invalid_params("recording_id must be a UUIDv7", None))?;
    if id.get_version_num() != 7 {
        return Err(McpError::invalid_params(
            "recording_id must be a UUIDv7",
            None,
        ));
    }
    Ok(RecordingId::from_uuid(id))
}

fn subscribable_recording_id(uri: &str) -> Result<RecordingId, McpError> {
    uris::parse_layers_uri(uri)
        .or_else(|| uris::parse_recording_uri(uri))
        .ok_or_else(|| McpError::invalid_params("resource is not subscribable", None))
        .and_then(parse_recording_id)
}

fn invalid_params(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

async fn ready(State(state): State<Arc<AppState>>) -> StatusCode {
    if let Err(error) = state.recordings.platform_store().healthcheck().await {
        tracing::warn!("recording MCP store readiness failed: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    if let Err(error) = state.recordings.storage_readiness() {
        tracing::warn!("recording MCP storage readiness failed: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    StatusCode::OK
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordingStorageDiagnostics {
    schema: &'static str,
    layer_cache: Option<veoveo_recording_mcp::layer_cache::LayerCacheStats>,
    projection_scratch: Option<veoveo_recording_mcp::service::ProjectionRuntimeStats>,
}

async fn storage_diagnostics(State(state): State<Arc<AppState>>) -> Response {
    let diagnostics = state
        .recordings
        .layer_cache_stats()
        .and_then(|layer_cache| {
            Ok(RecordingStorageDiagnostics {
                schema: "veoveo.io/recording-storage-diagnostics/v1",
                layer_cache,
                projection_scratch: state.recordings.projection_runtime_stats()?,
            })
        });
    match diagnostics {
        Ok(diagnostics) => Json(diagnostics).into_response(),
        Err(error) => {
            tracing::warn!(%error, "recording storage diagnostics failed");
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        }
    }
}

async fn playback_manifest(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path(recording_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let artifact_caller = match artifact_caller(identity.clone(), &headers) {
        Ok(caller) => caller,
        Err(error) => {
            tracing::warn!(%error, %recording_id, "recording playback omitted Artifact authority");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    let plan = match state
        .recordings
        .playback_plan(&identity, Some(&artifact_caller), recording_id)
        .await
    {
        Ok(Some(plan)) => plan,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording playback manifest failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let requested_grant = headers
        .get(RECORDING_GRANT_HEADER)
        .and_then(|value| value.to_str().ok());
    let grant = match state
        .recordings
        .issue_read_grant(
            &identity,
            plan.dataset_id,
            veoveo_platform_store::RecordingReadGrantClass::ViewerSegment,
            vec![plan.recording_id],
            plan.catalog_revision.clone(),
            requested_grant,
        )
        .await
    {
        Ok(grant) => grant,
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording viewer grant failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    match state.playback.prepare_manifest(plan, grant).await {
        Ok(manifest) => axum::Json(manifest).into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording playback catalog preparation failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn catalog_grant(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    headers: HeaderMap,
    Json(request): Json<CreateCatalogGrantRequest>,
) -> Response {
    let dataset_uuid = request.dataset_id;
    if dataset_uuid.get_version_num() != 7 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if request.recording_ids.is_empty() || request.recording_ids.len() > 500 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut recording_ids = Vec::with_capacity(request.recording_ids.len());
    for value in request.recording_ids {
        if value.get_version_num() != 7 {
            return StatusCode::BAD_REQUEST.into_response();
        }
        recording_ids.push(RecordingId::from_uuid(value));
    }
    recording_ids.sort_unstable();
    recording_ids.dedup();
    let dataset_id = veoveo_platform_store::RecordingDatasetId::from_uuid(dataset_uuid);
    let artifact_caller = match artifact_caller(identity.clone(), &headers) {
        Ok(caller) => caller,
        Err(error) => {
            tracing::warn!(%error, %dataset_id, "catalog grant omitted Artifact authority");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    let plans = match state
        .recordings
        .dataset_playback_plans(
            &identity,
            &artifact_caller,
            dataset_id,
            recording_ids.clone(),
        )
        .await
    {
        Ok(Some(plans)) => plans,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %dataset_id, "dataset catalog planning failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let catalog_revision = veoveo_recording_mcp::service::catalog_set_revision(&plans);
    let requested_grant = headers
        .get(RECORDING_GRANT_HEADER)
        .and_then(|value| value.to_str().ok());
    let grant = match state
        .recordings
        .issue_read_grant(
            &identity,
            dataset_id,
            veoveo_platform_store::RecordingReadGrantClass::CatalogDataset,
            recording_ids,
            catalog_revision,
            requested_grant,
        )
        .await
    {
        Ok(grant) => grant,
        Err(error) => {
            tracing::error!(%error, %dataset_id, "durable dataset catalog grant failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    match state.playback.prepare_catalog_grant(plans, grant).await {
        Ok(grant) => Json(grant).into_response(),
        Err(error) => {
            tracing::error!(%error, %dataset_id, "virtual dataset catalog failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn playback_live_recording(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path(recording_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let plan = match state
        .recordings
        .playback_plan(&identity, None, recording_id)
        .await
    {
        Ok(Some(plan)) => plan,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "live recording authorization failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let Some(live) = plan.live.as_ref() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        != Some(FRAMED_RRD_CONTENT_TYPE)
    {
        return StatusCode::NOT_ACCEPTABLE.into_response();
    }
    let Some(start) = headers
        .get(LIVE_RRD_START_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(LiveRrdStart::parse)
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    tracing::info!(
        %recording_id,
        layer_id = %live.descriptor.layer_id,
        current_byte_len = live.descriptor.current_byte_len,
        history_seconds = live.descriptor.history_seconds,
        video_preroll_seconds = live.descriptor.video_preroll_seconds,
        ?start,
        "governed Rerun channel playback opened"
    );
    let playback_store_id = match playback_store_id(plan.dataset_id, recording_id) {
        Ok(store_id) => store_id,
        Err(error) => {
            tracing::error!(%error, %recording_id, "live playback identity construction failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let stream = authorized_live_rrd_stream(
        state.recordings.clone(),
        identity,
        recording_id,
        state.recordings.live_history(),
        playback_store_id,
        start,
    );
    let mut response = Response::new(Body::from_stream(stream));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static(FRAMED_RRD_CONTENT_TYPE),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::HeaderName::from_static("x-accel-buffering"),
        header::HeaderValue::from_static("no"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    response
}

async fn playback_blueprint(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path((recording_id, revision)): Path<(String, u64)>,
    headers: HeaderMap,
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let artifact_caller = match artifact_caller(identity.clone(), &headers) {
        Ok(caller) => caller,
        Err(error) => {
            tracing::warn!(%error, %recording_id, "recording Blueprint omitted Artifact authority");
            return StatusCode::UNAUTHORIZED.into_response();
        }
    };
    let plan = match state
        .recordings
        .playback_plan(&identity, Some(&artifact_caller), recording_id)
        .await
    {
        Ok(Some(plan)) => plan,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording Blueprint authorization failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let Some(blueprint) = plan.blueprint.filter(|value| value.revision == revision) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let application_id = match playback_application_id(plan.dataset_id) {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, %recording_id, "Blueprint playback identity construction failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let result = tokio::task::spawn_blocking(move || {
        recording_scoped_blueprint(
            &blueprint.path,
            &application_id,
            &blueprint.blueprint_id,
            blueprint.byte_len,
            &blueprint.sha256,
        )
    })
    .await;
    match result {
        Ok(Ok(bytes)) => rrd_response(Body::from(bytes)),
        Ok(Err(error)) => {
            tracing::error!(%error, %recording_id, revision, "recording Blueprint playback failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(error) => {
            tracing::error!(%error, %recording_id, revision, "recording Blueprint worker failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn projection_data(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path((recording_id, projection_id)): Path<(String, String)>,
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(projection_uuid) = uuid::Uuid::parse_str(&projection_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if projection_uuid.get_version_num() != 7 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let projection_id = RecordingProjectionReceiptId::from_uuid(projection_uuid);
    let download = match state
        .recordings
        .projection_download(&identity, recording_id, projection_id)
        .await
    {
        Ok(Some(download)) => download,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, %projection_id, "recording projection redemption failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let file = match tokio::fs::File::open(&download.path).await {
        Ok(file) => file,
        Err(error) => {
            tracing::error!(%error, %recording_id, %projection_id, "recording projection result disappeared");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let stream = tokio_util::io::ReaderStream::new(file);
    let mut response = Response::new(Body::from_stream(stream));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/vnd.apache.arrow.stream"),
    );
    headers.insert(
        header::CONTENT_LENGTH,
        header::HeaderValue::from_str(&download.byte_len.to_string())
            .expect("u64 is a valid Content-Length"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("attachment; filename=recording-projection.arrow"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::HeaderName::from_static("x-veoveo-payload-sha256"),
        header::HeaderValue::from_str(&download.sha256)
            .expect("SHA-256 hex is a valid header value"),
    );
    response
}

fn rrd_response(body: Body) -> Response {
    let mut response = Response::new(body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/vnd.rerun.rrd"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_static("inline"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        header::HeaderValue::from_static("nosniff"),
    );
    response
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-recording-mcp", "info,veoveo_recording_mcp=debug")?;
    let args = Args::parse();
    let spool_dir = if args.spool_dir.is_absolute() {
        args.spool_dir.clone()
    } else {
        std::env::current_dir()?.join(&args.spool_dir)
    };
    let store = PlatformStore::connect(
        StoreConfig::builder(
            &args.surreal_endpoint,
            &args.surreal_namespace,
            &args.surreal_database,
            StoreCredentials::database(&args.surreal_username, args.surreal_password.clone()),
        )
        .build()?,
    )
    .await?;
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let state = Arc::new(AppState {
        recordings: RecordingService::new(
            store.clone(),
            HttpArtifactPlane::new(&args.artifact_service_url),
            spool_dir,
        )?
        .with_layer_cache(
            args.catalog_cache_dir,
            LayerCacheLimits {
                managed_bytes: args.catalog_cache_managed_bytes,
                minimum_free_bytes: args.catalog_cache_minimum_free_bytes,
            },
        )?
        .with_projection_runtime(ProjectionRuntimeLimits {
            aggregate_scratch_bytes: args.projection_scratch_bytes,
            minimum_free_bytes: args.projection_minimum_free_bytes,
            concurrent_projections: usize::from(args.projection_concurrency),
            maximum_deadline_ms: args.projection_deadline_ms,
        })?
        .with_layer_publisher(GatewayLayerPublisher::new(GatewayLayerPublisherConfig {
            gateway_url: args.gateway_url,
            gateway_transport_url: args.gateway_transport_url,
            protected_resource: args.publication_protected_resource,
            profile: args.publication_profile,
            client_id: args.publication_client_id,
            private_key_pem_file: args.publication_private_key_pem_file,
            key_id: args.publication_key_id,
            algorithm: args.publication_signing_algorithm,
        })?)
        .with_live_history_seconds(args.live_history_seconds)?,
        playback: PlaybackManager::new(
            args.playback_token_key.expose_secret(),
            &args.playback_public_url,
            store,
        )?,
        subscribers: SubscriptionHub::new(),
    });
    let cancellation = CancellationToken::new();
    let mut allowed_hosts: BTreeSet<String> = args.allowed_hosts.into_iter().collect();
    allowed_hosts.insert(format!("recording-mcp:{}", args.port));
    if args.allow_loopback_hosts {
        allowed_hosts.insert(format!("localhost:{}", args.port));
        allowed_hosts.insert(format!("127.0.0.1:{}", args.port));
    }
    let service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(RecordingMcp::new(state.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts)
            .with_cancellation_token(cancellation.child_token()),
    );
    let auth_state = InternalAuthState { verifier };
    let mcp = Router::new()
        .route_service("/", service.clone())
        .route_service("/{*path}", service)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate,
        ));
    let admin_router = admin::router().layer(middleware::from_fn_with_state(
        auth_state.clone(),
        authenticate,
    ));
    let storage_diagnostics_router = Router::new()
        .route("/admin/storage", get(storage_diagnostics))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate,
        ));
    let playback = Router::new()
        .route("/catalog-grants", post(catalog_grant))
        .route("/{recording_id}/playback", get(playback_manifest))
        .route(
            "/{recording_id}/live/rrd-stream",
            get(playback_live_recording),
        )
        .route(
            "/{recording_id}/blueprints/{revision}/data.rrd",
            get(playback_blueprint),
        )
        .route(
            "/{recording_id}/projections/{projection_id}/data.arrow",
            get(projection_data),
        )
        .layer(middleware::from_fn_with_state(auth_state, authenticate));
    let redap = tonic::service::Routes::new(RerunCloudServiceServer::new(
        state.playback.scoped_redap_service(),
    ))
    .into_axum_router()
    .layer(tonic_web::GrpcWebLayer::new())
    .with_state::<Arc<AppState>>(());
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .merge(storage_diagnostics_router)
        .nest_service("/admin", admin_router)
        .nest("/mcp", mcp)
        .nest("/recordings", playback)
        .merge(redap)
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );
    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(service = "veoveo-recording-mcp", %address, "listening");
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation.cancel();
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!RecordingMcp::tool_router().list_all().is_empty());
    }

    #[test]
    fn tools_publish_safety_annotations() {
        let tools = RecordingMcp::tool_router();
        let seal = tools
            .list_all()
            .into_iter()
            .find(|tool| tool.name == "seal_recording")
            .unwrap();
        let annotations = seal.annotations.unwrap();
        assert_eq!(annotations.read_only_hint, Some(false));
        assert_eq!(annotations.idempotent_hint, Some(true));
        assert_eq!(annotations.open_world_hint, Some(true));

        let projection = RecordingMcp::tool_router()
            .list_all()
            .into_iter()
            .find(|tool| tool.name == "create_recording_projection")
            .unwrap();
        let annotations = projection.annotations.unwrap();
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.idempotent_hint, Some(true));
        assert_eq!(annotations.open_world_hint, Some(true));
    }

    #[test]
    fn subscriptions_accept_only_recording_resources() {
        let id = uuid::Uuid::now_v7().to_string();
        assert!(subscribable_recording_id(&uris::recording_uri(&id)).is_ok());
        assert!(subscribable_recording_id(uris::CATALOG_URI).is_err());
    }

    #[test]
    fn contract_declaration_resolves_from_the_embedded_manual() {
        use veoveo_mcp_contract::docs::{CONTRACT_REVISION, ComplianceStatus};

        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        assert_eq!(declaration.server, "recording");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
