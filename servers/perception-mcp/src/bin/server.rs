//! Perception MCP server.
//!
//! Rerun remains the recording authority. This server resolves authorized
//! recording ranges, remuxes H.264 samples, invokes the configured DeepStream
//! runner, and publishes typed results plus immutable Rerun annotation layers.

use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Router, extract::State, http::StatusCode, middleware, routing::get};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use clap::Parser;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo,
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
use serde_json::json;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle, Page,
    ServerSlug, SubscriptionHub, TelemetryGuard, TokenIssuer, init_server_telemetry, paginate,
    public_allowed_hosts,
};
use veoveo_mcp_task_extension::{
    Implementation as TaskExtensionImplementation, ServerDiscovery, TaskExtensionAdapter,
    task_extension_middleware,
};
use veoveo_perception_mcp::{
    artifacts::ArtifactRepository,
    catalog::{PipelineCatalog, model_view, pipeline_view},
    contract::{
        AnalysisView, AnalyzeRecordingOutput, AnalyzeRecordingRequest, ExtractClipOutput,
        ExtractClipRequest,
    },
    executor::DeepStreamExecutor,
    source::VideoSourceLimits,
    uris,
};
use veoveo_platform_store::TaskStatus;
use veoveo_recording_mcp::RecordingService;
use veoveo_task_runtime::{TaskError, TaskRuntime, TaskRuntimeConfig, TaskSnapshot};

#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/config.rs"]
mod config;
#[path = "server/host.rs"]
mod host;
#[path = "server/internal_auth.rs"]
mod internal_auth;
#[path = "server/outputs.rs"]
mod outputs;
#[path = "server/ownership.rs"]
mod ownership;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/task_extension.rs"]
mod task_extension;
#[path = "server/tasks.rs"]
mod tasks;

use app_state::AppState;
use config::Args;
use host::validate_host;
use internal_auth::{InternalMcpAuthState, authenticate_internal_mcp};
use ownership::{internal_caller, internal_identity, require_task_owner, task_owner_allows};
use prompts::PerceptionPrompt;
use task_extension::PerceptionTaskExtension;
use tasks::{
    PerceptionTaskInput, SERVER_SLUG, TaskProgress, completed_payload, resume_task,
    start_perception_task,
};

const LIST_PAGE_SIZE: usize = 100;

#[derive(Clone)]
struct PerceptionMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<PerceptionMcp>,
}

#[tool_router]
impl PerceptionMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Analyze recorded video",
        description = "Resolve an authorized Rerun VideoStream range, run a configured DeepStream/TensorRT perception pipeline, and publish typed results plus an immutable Rerun annotation layer.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<AnalyzeRecordingOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn analyze_recording(
        &self,
        Parameters(request): Parameters<AnalyzeRecordingRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let snapshot = start_perception_task(
            self.state.clone(),
            internal_identity(&context)?,
            internal_caller(&context)?,
            PerceptionTaskInput::Analyze(request),
            Some(TaskProgress {
                peer: context.peer.clone(),
                token: context.meta.get_progress_token(),
            }),
            BTreeSet::new(),
        )
        .await
        .map_err(internal)?;
        let task_id = snapshot.task_id.to_string();
        let _ = context.peer.notify_resource_list_changed().await;
        completed_payload(&self.state, &task_id).await
    }

    #[tool(
        title = "Extract recorded clip",
        description = "Resolve an authorized Rerun VideoStream range from the preceding keyframe and publish an MP4 artifact without re-encoding.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ExtractClipOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn extract_clip(
        &self,
        Parameters(request): Parameters<ExtractClipRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let snapshot = start_perception_task(
            self.state.clone(),
            internal_identity(&context)?,
            internal_caller(&context)?,
            PerceptionTaskInput::ExtractClip(request),
            Some(TaskProgress {
                peer: context.peer.clone(),
                token: context.meta.get_progress_token(),
            }),
            BTreeSet::new(),
        )
        .await
        .map_err(internal)?;
        completed_payload(&self.state, &snapshot.task_id.to_string()).await
    }
}

#[tool_handler]
impl ServerHandler for PerceptionMcp {
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
            "Governed perception over Rerun recordings. Discover immutable model and pipeline identities through perception://models and perception://pipelines. Pass recording:// references and bounded timeline ranges to analyze_recording or extract_clip. Analyses are durable MCP tasks and publish perception://analysis resources plus governed artifacts."
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
        let identity = internal_identity(&context)?;
        let mut resources = vec![
            Resource::new(uris::PIPELINES_URI, "perception pipelines")
                .with_title("Perception pipelines")
                .with_description("Immutable analysis pipeline catalog.")
                .with_mime_type("application/json"),
            Resource::new(uris::MODELS_URI, "perception models")
                .with_title("Perception models")
                .with_description("Immutable model catalog without private filesystem details.")
                .with_mime_type("application/json"),
            Resource::new(uris::ANALYSES_URI, "perception analyses")
                .with_title("Perception analyses")
                .with_description("Authorized durable analysis index.")
                .with_mime_type("application/json"),
        ];
        for pipeline in self.state.catalog.pipeline_views() {
            resources.push(
                Resource::new(pipeline.uri, format!("pipeline {}", pipeline.id))
                    .with_title(pipeline.title)
                    .with_description(pipeline.description)
                    .with_mime_type("application/json"),
            );
        }
        for model in self.state.catalog.model_views() {
            resources.push(
                Resource::new(model.uri, format!("model {}", model.id))
                    .with_title(model.title)
                    .with_description(model.description)
                    .with_mime_type("application/json"),
            );
        }
        for snapshot in visible_analyses(&self.state, &identity).await? {
            let task_id = snapshot.task_id.to_string();
            resources.push(
                Resource::new(uris::analysis_uri(&task_id), format!("analysis {task_id}"))
                    .with_title(format!("Perception analysis {task_id}"))
                    .with_description("Durable task state and artifact identities.")
                    .with_mime_type("application/json"),
            );
            resources.push(
                Resource::new(uris::results_uri(&task_id), format!("results {task_id}"))
                    .with_title(format!("Perception results {task_id}"))
                    .with_description("Typed detections for one completed analysis.")
                    .with_mime_type("application/vnd.veoveo.perception-results+json"),
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
            ResourceTemplate::new(uris::PIPELINE_TEMPLATE, "pipeline")
                .with_title("Perception pipeline")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::MODEL_TEMPLATE, "model")
                .with_title("Perception model")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::ANALYSIS_TEMPLATE, "analysis")
                .with_title("Perception analysis")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::RESULTS_TEMPLATE, "analysis results")
                .with_title("Perception analysis results")
                .with_mime_type("application/vnd.veoveo.perception-results+json"),
            ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
                .with_title("Perception artifact"),
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
        let uri = request.uri.as_str();
        if uri == uris::PIPELINES_URI {
            return json_resource(uri, &self.state.catalog.pipeline_views());
        }
        if uri == uris::MODELS_URI {
            return json_resource(uri, &self.state.catalog.model_views());
        }
        if let Some(id) = uris::parse_pipeline_uri(uri) {
            let pipeline = self
                .state
                .catalog
                .pipeline(id)
                .map(pipeline_view)
                .ok_or_else(|| McpError::resource_not_found("pipeline not found", None))?;
            return json_resource(uri, &pipeline);
        }
        if let Some(id) = uris::parse_model_uri(uri) {
            let model = self
                .state
                .catalog
                .model(id)
                .map(model_view)
                .ok_or_else(|| McpError::resource_not_found("model not found", None))?;
            return json_resource(uri, &model);
        }
        let identity = internal_identity(&context)?;
        if uri == uris::ANALYSES_URI {
            let views = visible_analyses(&self.state, &identity)
                .await?
                .iter()
                .map(analysis_view)
                .collect::<Result<Vec<_>, _>>()?;
            return json_resource(uri, &views);
        }
        if let Some(task_id) = uris::parse_analysis_uri(uri) {
            require_task_owner(&self.state, &context, task_id).await?;
            let snapshot = analysis_snapshot(&self.state, task_id).await?;
            return json_resource(uri, &analysis_view(&snapshot)?);
        }
        if let Some(task_id) = uris::parse_results_uri(uri) {
            require_task_owner(&self.state, &context, task_id).await?;
            let snapshot = analysis_snapshot(&self.state, task_id).await?;
            let output = analysis_output(&snapshot).ok_or_else(|| {
                McpError::resource_not_found("analysis results are not available", None)
            })?;
            let caller = internal_caller(&context)?;
            let artifact =
                inline_artifact(&self.state, &caller, &output.results_artifact.artifact_id).await?;
            let text = String::from_utf8(artifact.bytes)
                .map_err(|_| McpError::internal_error("results artifact is not UTF-8", None))?;
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(text, uri)
                    .with_mime_type("application/vnd.veoveo.perception-results+json"),
            ]));
        }
        if let Some(artifact_id) = uris::parse_artifact_uri(uri) {
            let caller = internal_caller(&context)?;
            let artifact = inline_artifact(&self.state, &caller, &artifact_id).await?;
            let mut content = ResourceContents::blob(BASE64_STANDARD.encode(artifact.bytes), uri);
            if let Some(mime_type) = artifact.metadata.mime_type {
                content = content.with_mime_type(mime_type);
            }
            return Ok(ReadResourceResult::new(vec![content]));
        }
        Err(McpError::resource_not_found(
            format!("unknown perception resource `{uri}`"),
            None,
        ))
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts: Vec<Prompt> = PerceptionPrompt::ALL
            .into_iter()
            .map(PerceptionPrompt::definition)
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
        PerceptionPrompt::by_name(&request.name)
            .ok_or_else(|| McpError::invalid_params("unknown perception prompt", None))?
            .render(request.arguments)
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let task_id = subscribable_analysis_id(&request.uri)?;
        require_task_owner(&self.state, &context, task_id).await?;
        let identity = internal_identity(&context)?;
        self.state
            .subscribers
            .subscribe(request.uri, identity.principal.id, context.peer.clone())
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let task_id = subscribable_analysis_id(&request.uri)?;
        require_task_owner(&self.state, &context, task_id).await?;
        let identity = internal_identity(&context)?;
        self.state
            .subscribers
            .unsubscribe(&request.uri, &identity.principal.id)
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
        let values = match (reference.uri.as_str(), request.argument.name.as_str()) {
            (uris::PIPELINE_TEMPLATE, "pipeline_id") => self.state.catalog.pipeline_ids(),
            (uris::MODEL_TEMPLATE, "model_id") => self.state.catalog.model_ids(),
            (uris::ANALYSIS_TEMPLATE | uris::RESULTS_TEMPLATE, "analysis_id") => {
                let identity = internal_identity(&context)?;
                visible_analyses(&self.state, &identity)
                    .await?
                    .into_iter()
                    .map(|snapshot| snapshot.task_id.to_string())
                    .collect()
            }
            (uris::ARTIFACT_TEMPLATE, "artifact_id") => {
                let identity = internal_identity(&context)?;
                visible_analyses(&self.state, &identity)
                    .await?
                    .iter()
                    .filter_map(analysis_output)
                    .flat_map(|output| {
                        let mut ids = vec![
                            output.results_artifact.artifact_id.to_string(),
                            output.annotations_artifact.artifact_id.to_string(),
                        ];
                        if let Some(artifact) = output.source_clip_artifact {
                            ids.push(artifact.artifact_id.to_string());
                        }
                        ids
                    })
                    .collect()
            }
            _ => return Ok(CompleteResult::default()),
        };
        let needle = request.argument.value.to_lowercase();
        let matches = values
            .into_iter()
            .filter(|value| value.contains(&needle))
            .collect::<Vec<_>>();
        let total = matches.len();
        let completion = CompletionInfo::with_pagination(
            matches
                .into_iter()
                .take(CompletionInfo::MAX_VALUES)
                .collect(),
            Some(total as u32),
            total > CompletionInfo::MAX_VALUES,
        )
        .map_err(internal)?;
        Ok(CompleteResult::new(completion))
    }
}

async fn visible_analyses(
    state: &AppState,
    identity: &veoveo_mcp_contract::GatewayInternalIdentity,
) -> Result<Vec<TaskSnapshot>, McpError> {
    let mut snapshots = state.tasks.list().await.map_err(internal)?;
    snapshots.retain(|snapshot| {
        snapshot.task_type == "analyze_recording" && task_owner_allows(&snapshot.owner, identity)
    });
    snapshots.sort_by_key(|snapshot| snapshot.created_at);
    Ok(snapshots)
}

async fn analysis_snapshot(state: &AppState, task_id: &str) -> Result<TaskSnapshot, McpError> {
    let snapshot = state
        .tasks
        .get(task_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| McpError::resource_not_found("analysis not found", None))?;
    if snapshot.task_type != "analyze_recording" {
        return Err(McpError::resource_not_found("analysis not found", None));
    }
    Ok(snapshot)
}

fn analysis_view(snapshot: &TaskSnapshot) -> Result<AnalysisView, McpError> {
    let request: tasks::DurablePerceptionRequest =
        serde_json::from_value(snapshot.request.clone()).map_err(internal)?;
    let PerceptionTaskInput::Analyze(input) = request.input else {
        return Err(McpError::resource_not_found("analysis not found", None));
    };
    Ok(AnalysisView {
        analysis_uri: uris::analysis_uri(&snapshot.task_id.to_string()),
        results_uri: uris::results_uri(&snapshot.task_id.to_string()),
        task_id: snapshot.task_id.to_string(),
        status: task_status(snapshot.status).to_owned(),
        progress: snapshot.progress,
        pipeline_id: input.pipeline_id,
        recording_uri: input.video.recording_uri,
        entity_path: input.video.entity_path,
        timeline: input.video.timeline,
        created_at: snapshot.created_at.to_rfc3339(),
        updated_at: snapshot.updated_at.to_rfc3339(),
        output: analysis_output(snapshot),
        error: snapshot.error.as_ref().map(|error| error.message.clone()),
    })
}

fn analysis_output(snapshot: &TaskSnapshot) -> Option<AnalyzeRecordingOutput> {
    let result = serde_json::from_value::<CallToolResult>(snapshot.result.clone()?).ok()?;
    serde_json::from_value(result.structured_content?).ok()
}

fn task_status(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Waiting => "waiting",
        TaskStatus::Succeeded => "succeeded",
        TaskStatus::Failed => "failed",
        TaskStatus::CancelRequested => "cancel_requested",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn subscribable_analysis_id(uri: &str) -> Result<&str, McpError> {
    uris::parse_analysis_uri(uri)
        .or_else(|| uris::parse_results_uri(uri))
        .ok_or_else(|| McpError::invalid_params("resource is not subscribable", None))
}

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE).map_err(invalid_params)
}

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, McpError> {
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(serde_json::to_string(value).map_err(internal)?, uri)
            .with_mime_type("application/json"),
    ]))
}

fn invalid_params(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

async fn inline_artifact(
    state: &AppState,
    caller: &veoveo_mcp_contract::PlaneCaller,
    artifact_id: &veoveo_mcp_contract::ArtifactId,
) -> Result<veoveo_mcp_contract::ArtifactObject, McpError> {
    let metadata = state
        .artifacts
        .head(caller, artifact_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| McpError::resource_not_found("artifact not found", None))?;
    if metadata.byte_len > state.max_inline_resource_bytes {
        return Err(McpError::invalid_request(
            format!(
                "artifact is {} bytes and exceeds the {}-byte inline MCP resource limit; use its governed artifact download path",
                metadata.byte_len, state.max_inline_resource_bytes
            ),
            None,
        ));
    }
    let artifact = state
        .artifacts
        .get(caller, artifact_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| McpError::resource_not_found("artifact not found", None))?;
    if artifact.bytes.len() as u64 != metadata.byte_len
        || artifact.bytes.len() as u64 > state.max_inline_resource_bytes
    {
        return Err(McpError::internal_error(
            "artifact byte length changed while reading inline resource",
            None,
        ));
    }
    Ok(artifact)
}

async fn ready(State(state): State<Arc<AppState>>) -> StatusCode {
    if let Err(error) = state.tasks.platform_store().healthcheck().await {
        tracing::warn!("perception readiness database failure: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    if let Err(error) = state.executor.readiness() {
        tracing::warn!("perception readiness runner failure: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    StatusCode::OK
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-perception-mcp", "info,veoveo_perception_mcp=debug")?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            args.surreal_endpoint.clone(),
            args.surreal_namespace.clone(),
            args.surreal_database.clone(),
            args.surreal_auth_level,
            args.surreal_username.clone(),
            args.surreal_password.clone(),
        ),
        SERVER_SLUG,
        format!("{SERVER_SLUG}-{}", uuid::Uuid::now_v7()),
    )
    .await?;
    let recovery = tasks.recover().await?;
    let spool_dir = if args.spool_dir.is_absolute() {
        args.spool_dir.clone()
    } else {
        std::env::current_dir()?.join(&args.spool_dir)
    };
    let recordings = Arc::new(RecordingService::new(
        tasks.platform_store().clone(),
        HttpArtifactPlane::new(&args.artifact_service_url),
        spool_dir,
    )?);
    let catalog = Arc::new(PipelineCatalog::load(&args.pipeline_catalog)?);
    let executor = DeepStreamExecutor::new(
        args.deepstream_runner.clone(),
        args.runner_timeout(),
        args.max_result_frames,
        args.max_detections_per_frame,
        args.max_runner_response_bytes,
    )?;
    executor.readiness()?;
    let source_limits = VideoSourceLimits {
        max_samples: args.max_video_samples,
        max_encoded_bytes: args.max_encoded_video_bytes,
        max_segment_bytes: args.max_segment_bytes,
    };
    source_limits.validate()?;
    anyhow::ensure!(
        args.max_artifact_bytes > 0,
        "max_artifact_bytes must be non-zero"
    );
    anyhow::ensure!(
        args.max_inline_resource_bytes > 0,
        "max_inline_resource_bytes must be non-zero"
    );
    anyhow::ensure!(
        args.max_concurrent_jobs > 0,
        "max_concurrent_jobs must be non-zero"
    );
    let state = Arc::new(AppState {
        tasks,
        artifacts: ArtifactRepository::new(args.artifact_service_url.clone()),
        recordings,
        catalog,
        executor,
        source_limits,
        max_artifact_bytes: args.max_artifact_bytes,
        max_inline_resource_bytes: args.max_inline_resource_bytes,
        work_slots: Arc::new(tokio::sync::Semaphore::new(args.max_concurrent_jobs)),
        subscribers: SubscriptionHub::new(),
    });
    for snapshot in recovery.resumable {
        if let Err(error) = resume_task(state.clone(), snapshot).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(task_id, "another replica claimed recovered perception task");
                }
                _ => return Err(error),
            }
        }
    }

    let cancellation = CancellationToken::new();
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts.into_iter().collect::<Vec<_>>());
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(PerceptionMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_stateful_mode(false)
            .with_json_response(true)
            .with_cancellation_token(cancellation.child_token()),
    );
    let task_extension = Arc::new(TaskExtensionAdapter::new(
        Arc::new(PerceptionTaskExtension::new(state.clone())),
        ServerDiscovery::new(
            HashMap::from([
                ("tools".to_owned(), json!({})),
                ("resources".to_owned(), json!({})),
                ("prompts".to_owned(), json!({})),
            ])
            .into_iter()
            .collect(),
            TaskExtensionImplementation {
                name: SERVER_SLUG.to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some(
                "Governed Rerun video analysis with durable tasks and NVIDIA DeepStream/TensorRT execution."
                    .to_owned(),
            ),
        ),
    ));
    let auth_state = InternalMcpAuthState { verifier };
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            task_extension,
            task_extension_middleware::<PerceptionTaskExtension>,
        ))
        .layer(middleware::from_fn_with_state(
            auth_state,
            authenticate_internal_mcp,
        ));
    let service_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .with_state(state.clone())
        .nest("/mcp", mcp_router);
    let router = Router::new()
        .nest(public_endpoint.mount_path(), service_router)
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );
    let address = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(
        service = "veoveo-perception-mcp",
        %address,
        mcp_path = public_endpoint.path("mcp"),
        public_url = public_endpoint.public_url(),
        "listening"
    );
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            cancellation.cancel();
        })
        .await?;
    Ok(())
}
