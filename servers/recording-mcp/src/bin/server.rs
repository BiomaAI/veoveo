use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Extension, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use clap::Parser;
use re_protos::cloud::v1alpha1::rerun_cloud_service_server::RerunCloudServiceServer;
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock,
        GetPromptRequestParams, GetPromptResult, ListPromptsResult, ListResourceTemplatesResult,
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
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle, Page,
    ServerSlug, SubscriptionHub, TelemetryGuard, TokenIssuer, init_server_telemetry, paginate,
};
use veoveo_platform_store::{PlatformStore, RecordingId, StoreConfig, StoreCredentials};
use veoveo_recording_mcp::blueprint_playback::recording_scoped_blueprint;
use veoveo_recording_mcp::live_stream::{
    FRAMED_RRD_CONTENT_TYPE, LIVE_RRD_START_HEADER, LiveRrdStart, authorized_live_rrd_stream,
};
use veoveo_recording_mcp::{
    RecordingService,
    admin::{self, SERVER_DOCS},
    contract::{
        QueryRecordingOutput, QueryRecordingRequest, SealRecordingOutput, SealRecordingRequest,
    },
    playback::{
        PLAYBACK_SESSION_HEADER, PlaybackManager, playback_application_id, playback_store_id,
    },
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

use auth::{InternalAuthState, authenticate, caller, identity};
use config::Args;
use prompts::RecordingPrompt;
use state::AppState;

const SERVER_SLUG: &str = "recording";
const LIST_PAGE_SIZE: usize = 100;

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
        title = "Query recording",
        description = "Run a row-bounded snapshot query over the authorized durable RRD segments of one recording, optionally within an inclusive timeline range.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<QueryRecordingOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn query_recording(
        &self,
        Parameters(request): Parameters<QueryRecordingRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = identity(&context)?;
        let output = self
            .state
            .recordings
            .query(&identity, request)
            .await
            .map_err(invalid_params)?;
        structured_result(format!("returned {} row(s)", output.rows.len()), &output)
    }

    #[tool(
        title = "Seal recording",
        description = "Fsync and validate every frozen segment, publish governed immutable segment and manifest artifacts, then atomically seal the recording. Requires admin:manage scope.",
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
        let caller = caller(&context)?;
        let output = self
            .state
            .recordings
            .seal(&identity, &caller, recording_id)
            .await
            .map_err(invalid_params)?;
        self.state
            .subscribers
            .notify_resource_updated(uris::recording_uri(&request.recording_id))
            .await;
        self.state
            .subscribers
            .notify_resource_updated(uris::segments_uri(&request.recording_id))
            .await;
        self.state.subscribers.notify_resource_list_changed().await;
        structured_result("recording sealed".to_owned(), &output)
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
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new(SERVER_SLUG, env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Governed access to the installation recording catalog. Discover recordings through resources, query bounded temporal rows from frozen shards or acknowledged live parts with query_recording, and seal only frozen recordings when the caller has admin:manage scope. Sealing returns artifact:// occurrence URIs; artifact policy controls subsequent reads and sharing."
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
                    uris::segments_uri(&recording.recording_id),
                    format!("segments for {}", recording.recording_key),
                )
                .with_title(format!("Segments for {}", recording.recording_key))
                .with_description("Durable segment validation and artifact state.")
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
            ResourceTemplate::new(uris::SEGMENTS_TEMPLATE, "recording segments")
                .with_title("Recording segments")
                .with_description("Durable segments for one governed recording.")
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
            if let Some(value) = uris::parse_segments_uri(uri) {
                let recording_id = parse_recording_id(value)?;
                let segments = self
                    .state
                    .recordings
                    .segment_views(&identity, recording_id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| McpError::resource_not_found("recording not found", None))?;
                return json_resource(uri, &segments);
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
            uris::RECORDING_TEMPLATE | uris::SEGMENTS_TEMPLATE
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
    uris::parse_segments_uri(uri)
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
    match state.recordings.platform_store().healthcheck().await {
        Ok(()) => StatusCode::OK,
        Err(error) => {
            tracing::warn!("recording MCP readiness failed: {error}");
            StatusCode::SERVICE_UNAVAILABLE
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
    let plan = match state
        .recordings
        .playback_plan(&identity, recording_id)
        .await
    {
        Ok(Some(plan)) => plan,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording playback manifest failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let requested_session = headers
        .get(PLAYBACK_SESSION_HEADER)
        .and_then(|value| value.to_str().ok());
    match state
        .playback
        .prepare_manifest(&identity, plan, requested_session)
        .await
    {
        Ok(manifest) => axum::Json(manifest).into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording playback catalog preparation failed");
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
        .playback_plan(&identity, recording_id)
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
        segment_id = %live.descriptor.segment_id,
        current_byte_len = live.descriptor.current_byte_len,
        history_seconds = live.descriptor.history_seconds,
        video_preroll_seconds = live.descriptor.video_preroll_seconds,
        ?start,
        "governed Rerun channel playback opened"
    );
    let playback_store_id = match playback_store_id(recording_id, &plan.recording_key) {
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
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let plan = match state
        .recordings
        .playback_plan(&identity, recording_id)
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
    let application_id = match playback_application_id(recording_id) {
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
            store,
            HttpArtifactPlane::new(&args.artifact_service_url),
            spool_dir,
        )?
        .with_live_history_seconds(args.live_history_seconds)?,
        playback: PlaybackManager::new(
            args.playback_token_key.expose_secret(),
            &args.playback_public_url,
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
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate,
        ));
    let admin_router = admin::router().layer(middleware::from_fn_with_state(
        auth_state.clone(),
        authenticate,
    ));
    let playback = Router::new()
        .route("/{recording_id}/playback", get(playback_manifest))
        .route(
            "/{recording_id}/live/rrd-stream",
            get(playback_live_recording),
        )
        .route(
            "/{recording_id}/blueprints/{revision}/data.rrd",
            get(playback_blueprint),
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
