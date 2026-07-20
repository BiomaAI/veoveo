use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Extension, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Method, Request, StatusCode, header},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use clap::Parser;
use futures::stream;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock,
        GetPromptRequestParams, GetPromptResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt,
        ReadResourceRequestParams, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerInfo, SubscribeRequestParams,
        UnsubscribeRequestParams,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::Serialize;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle, Page,
    ServerSlug, SubscriptionHub, TelemetryGuard, TokenIssuer, init_server_telemetry, paginate,
};
use veoveo_platform_store::{PlatformStore, RecordingId, SegmentId, StoreConfig, StoreCredentials};
use veoveo_recording_mcp::live_playback::stream_live_rrd;
use veoveo_recording_mcp::{
    RecordingService,
    contract::{
        QueryRecordingOutput, QueryRecordingRequest, SealRecordingOutput, SealRecordingRequest,
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
        description = "Run a bounded snapshot query over the authorized durable RRD segments of one recording.",
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
        let _ = context.peer.notify_resource_list_changed().await;
        structured_result("recording sealed".to_owned(), &output)
    }
}

#[tool_handler]
impl ServerHandler for RecordingMcp {
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
            "Governed access to the installation recording catalog. Discover recordings through resources, query bounded temporal rows with query_recording, and seal only frozen recordings when the caller has admin:manage scope. Sealing returns artifact:// occurrence URIs; artifact policy controls subsequent reads and sharing."
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
        let identity = identity(&context)?;
        let mut resources = vec![
            Resource::new(uris::CATALOG_URI, "recording catalog")
                .with_title("Recording catalog")
                .with_description("Authorized recording lifecycle and artifact index.")
                .with_mime_type("application/json"),
        ];
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
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let templates = vec![
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
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = identity(&context)?;
        let uri = request.uri.as_str();
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
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        RecordingPrompt::by_name(&request.name)
            .ok_or_else(|| McpError::invalid_params("unknown recording prompt", None))?
            .render(request.arguments)
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = identity(&context)?;
        let recording_id = subscribable_recording_id(&request.uri)?;
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
        self.state
            .subscribers
            .subscribe(request.uri, identity.actor.id, context.peer.clone())
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = identity(&context)?;
        let recording_id = subscribable_recording_id(&request.uri)?;
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
        self.state
            .subscribers
            .unsubscribe(&request.uri, &identity.actor.id)
            .await;
        Ok(())
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
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match state
        .recordings
        .playback_manifest(&identity, recording_id)
        .await
    {
        Ok(Some(manifest)) => axum::Json(manifest).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, "recording playback manifest failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn playback_segment(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path((recording_id, segment_id)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(segment_id) = uuid::Uuid::parse_str(&segment_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if segment_id.get_version_num() != 7 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = match state
        .recordings
        .playback_segment_path(&identity, recording_id, SegmentId::from_uuid(segment_id))
        .await
    {
        Ok(Some(path)) => path,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, %segment_id, "recording segment authorization failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let mut request = Request::builder().method(method).uri("/");
    for name in [
        header::RANGE,
        header::IF_RANGE,
        header::IF_MATCH,
        header::IF_NONE_MATCH,
        header::IF_MODIFIED_SINCE,
        header::IF_UNMODIFIED_SINCE,
    ] {
        if let Some(value) = headers.get(&name) {
            request = request.header(name, value);
        }
    }
    let request = match request.body(Body::empty()) {
        Ok(request) => request,
        Err(error) => {
            tracing::error!(%error, "failed to build recording segment request");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    match ServeFile::new(path).oneshot(request).await {
        Ok(mut response) => {
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/vnd.rerun.rrd"),
            );
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                header::HeaderValue::from_static("private, no-store"),
            );
            response.map(Body::new)
        }
        Err(error) => {
            tracing::error!(%error, "recording segment read failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn playback_live_segment(
    State(state): State<Arc<AppState>>,
    Extension(identity): Extension<veoveo_mcp_contract::GatewayInternalIdentity>,
    Path((recording_id, segment_id)): Path<(String, String)>,
) -> Response {
    let Ok(recording_id) = parse_recording_id(&recording_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(segment_id) = uuid::Uuid::parse_str(&segment_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if segment_id.get_version_num() != 7 {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = match state
        .recordings
        .playback_live_segment_path(&identity, recording_id, SegmentId::from_uuid(segment_id))
        .await
    {
        Ok(Some(path)) => path,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::error!(%error, %recording_id, %segment_id, "live recording segment authorization failed");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let stream = stream::unfold(
        stream_live_rrd(path, state.recordings.live_history()),
        |mut receiver| async move { receiver.recv().await.map(|item| (item, receiver)) },
    );
    let mut response = rrd_response(Body::from_stream(stream));
    response.headers_mut().insert(
        header::HeaderName::from_static("x-accel-buffering"),
        header::HeaderValue::from_static("no"),
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
            store,
            HttpArtifactPlane::new(&args.artifact_service_url),
            spool_dir,
        )?
        .with_live_history_seconds(args.live_history_seconds)?,
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
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
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
    let playback = Router::new()
        .route("/{recording_id}/playback", get(playback_manifest))
        .route(
            "/{recording_id}/segments/{segment_id}/data.rrd",
            get(playback_segment),
        )
        .route(
            "/{recording_id}/segments/{segment_id}/live.rrd",
            get(playback_live_segment),
        )
        .layer(middleware::from_fn_with_state(auth_state, authenticate));
    let router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .nest("/mcp", mcp)
        .nest("/recordings", playback)
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
}
