//! Reason MCP server.
//!
//! Rerun remains the recording authority. This server resolves authorized
//! recording ranges, remuxes H.264 samples, invokes the configured world-model
//! runner, and publishes typed reasoning results plus immutable Rerun
//! annotation layers.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock};

use axum::{Router, extract::State, http::StatusCode, middleware, routing::get};
use clap::Parser;
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, CancelTaskParams,
        CompleteRequestParams, CompleteResult, CompletionInfo, GetPromptRequestParams,
        GetTaskParams, GetTaskResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt,
        ReadResourceRequestParams, Reference, ServerConfig, SubscriptionFilter, UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
    transport::streamable_http_server::StreamableHttpService,
};
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle, Page,
    ServerSlug, TelemetryGuard, TokenIssuer, init_server_telemetry, paginate, public_allowed_hosts,
};
use veoveo_reason_mcp::{
    artifacts::ArtifactRepository,
    catalog::PipelineCatalog,
    contract::{AnalyzeRecordingOutput, AnalyzeRecordingRequest},
    executor::ReasonExecutor,
    uris,
};
use veoveo_recording_reader::RecordingReader;
use veoveo_recording_video::runtime::VideoSourceLimits;
use veoveo_task_runtime::{TaskError, TaskRuntime, TaskRuntimeConfig};

#[path = "server/admin.rs"]
mod admin;
#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/config.rs"]
mod config;
#[path = "server/host.rs"]
mod host;
#[path = "server/index.rs"]
mod index;
#[path = "server/internal_auth.rs"]
mod internal_auth;
#[path = "server/outputs.rs"]
mod outputs;
#[path = "server/ownership.rs"]
mod ownership;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/resources.rs"]
mod resources;
#[path = "server/setup.rs"]
mod setup;
#[cfg(test)]
#[path = "../../../../testing/fixtures/store.rs"]
mod store_fixture;
#[path = "server/task_extension.rs"]
mod task_extension;
#[path = "server/task_results.rs"]
mod task_results;
#[path = "server/tasks.rs"]
mod tasks;

use app_state::AppState;
use config::Args;
use host::validate_host;
use internal_auth::{InternalMcpAuthState, authenticate_internal_mcp};
use ownership::{internal_caller, internal_identity, runtime_owner};
use prompts::ReasonPrompt;
use task_extension::ReasonTaskService;
use tasks::{
    ReasonTaskInput, SERVER_SLUG, TaskProgress, completed_payload, resume_task, start_reason_task,
};
use veoveo_reason_mcp::contract::AnalysisId;

const LIST_PAGE_SIZE: usize = 100;

use setup::SERVER_DOCS;

#[derive(Clone)]
struct ReasonMcp {
    state: Arc<AppState>,
    task_service: ReasonTaskService,
    #[allow(dead_code)]
    tool_router: ToolRouter<ReasonMcp>,
}

#[tool_router]
impl ReasonMcp {
    fn new(state: Arc<AppState>) -> Self {
        LazyLock::force(&setup::SERVER_SETUP);
        setup::catalog_resources(&state.catalog).expect("validated Reason catalog descriptors");
        Self {
            task_service: ReasonTaskService::new(state.clone()),
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Reason over recorded video",
        description = "Describe a video range from a recording you can read, detect events in it, or answer a question about it, using a configured world-model reasoning pipeline. Publishes typed results and a Rerun annotation layer.",
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
        let snapshot = start_reason_task(
            self.state.clone(),
            internal_identity(&context)?,
            internal_caller(&context)?,
            ReasonTaskInput::Analyze(request),
            Some(TaskProgress {
                peer: context.peer.clone(),
                token: context.meta.get_progress_token(),
            }),
            BTreeSet::new(),
        )
        .await
        .map_err(internal)?;
        let task_id = AnalysisId::try_from(snapshot.task_id).map_err(internal)?;
        completed_payload(&self.state, task_id).await
    }
}

#[tool_handler]
impl ServerHandler for ReasonMcp {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }

    fn get_info(&self) -> ServerConfig {
        setup::SERVER_SETUP.server_config().clone()
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
        tools = tools
            .into_iter()
            .map(|tool| {
                veoveo_mcp_apps_extension::link_tool_to_app(
                    tool,
                    uris::ANALYSES_APP_URI,
                    &[
                        veoveo_mcp_apps_extension::UiVisibility::Model,
                        veoveo_mcp_apps_extension::UiVisibility::App,
                    ],
                )
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
        internal_identity(&context)?;
        let mut resources = setup::SERVER_SETUP
            .resources()
            .iter()
            .map(|resource| resource.descriptor().clone())
            .collect::<Vec<_>>();
        resources.extend(setup::catalog_resources(&self.state.catalog).map_err(internal)?);
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
        let templates = setup::SERVER_SETUP
            .resource_templates()
            .iter()
            .map(|template| template.descriptor().clone())
            .collect();
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
        resources::read(&self.state, &request.uri, &context)
            .await
            .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts: Vec<Prompt> = ReasonPrompt::ALL
            .into_iter()
            .map(ReasonPrompt::definition)
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
            ReasonPrompt::by_name(&request.name)
                .ok_or_else(|| McpError::invalid_params("unknown reason prompt", None))?
                .render(request.arguments)
        }
        .await
        .map(Into::into)
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        resources::accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let subscriptions = veoveo_task_runtime::TaskResourceSubscriptions::from_filter::<
            veoveo_reason_mcp::contract::AnalysisResource,
        >(context.accepted())?;
        let identity = internal_identity(context.request_context())?;
        for task_id in subscriptions.resource_task_ids() {
            resources::analysis_snapshot(
                &self.state.tasks,
                &runtime_owner(&identity),
                AnalysisId::try_from(task_id).map_err(invalid_params)?,
            )
            .await?;
        }
        subscriptions.listen(&self.task_service, context).await
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
            (uris::PIPELINE_TEMPLATE, "pipeline_id") => self
                .state
                .catalog
                .pipeline_ids()
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>(),
            (uris::MODEL_TEMPLATE, "model_id") => self
                .state
                .catalog
                .model_ids()
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>(),
            (uris::ANALYSIS_TEMPLATE | uris::RESULTS_TEMPLATE, "analysis_id") => {
                let identity = internal_identity(&context)?;
                return index::complete(
                    &self.state.tasks,
                    &runtime_owner(&identity),
                    index::CompletionDomain::Analyses,
                    &request.argument.value,
                )
                .await
                .map(CompleteResult::new);
            }
            (uris::ARTIFACT_TEMPLATE, "artifact_id") => {
                let identity = internal_identity(&context)?;
                return index::complete(
                    &self.state.tasks,
                    &runtime_owner(&identity),
                    index::CompletionDomain::Artifacts,
                    &request.argument.value,
                )
                .await
                .map(CompleteResult::new);
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

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE).map_err(invalid_params)
}

fn invalid_params(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

async fn ready(State(state): State<Arc<AppState>>) -> StatusCode {
    if let Err(error) = state.recordings.readiness() {
        tracing::warn!("recording cache readiness failure: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    if let Err(error) = state.tasks.platform_store().healthcheck().await {
        tracing::warn!("reason readiness database failure: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    if let Err(error) = state.executor.readiness() {
        tracing::warn!("reason readiness runner failure: {error}");
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    StatusCode::OK
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    LazyLock::force(&setup::SERVER_SETUP);
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-reason-mcp", "info,veoveo_reason_mcp=debug")?;
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
    let recordings = Arc::new(RecordingReader::new(
        tasks.platform_store().clone(),
        spool_dir,
        veoveo_recording_reader::cache::LayerCache::new(
            args.catalog_cache_dir.clone(),
            veoveo_recording_reader::cache::LayerCacheLimits {
                managed_bytes: args.catalog_cache_managed_bytes,
                minimum_free_bytes: args.catalog_cache_minimum_free_bytes,
            },
            veoveo_artifact_client::HttpArtifactPlane::new(&args.artifact_service_url),
        )?,
    )?);
    let catalog = Arc::new(PipelineCatalog::load(&args.pipeline_catalog)?);
    let executor = ReasonExecutor::new(
        args.reason_runner.clone(),
        args.runner_timeout(),
        args.max_events,
        args.max_answer_bytes,
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
        args.max_grounding_bytes > 0,
        "max_grounding_bytes must be non-zero"
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
        max_grounding_bytes: args.max_grounding_bytes,
        work_slots: Arc::new(tokio::sync::Semaphore::new(args.max_concurrent_jobs)),
    });
    for snapshot in recovery.resumable {
        if let Err(error) = resume_task(state.clone(), snapshot).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(task_id, "another replica claimed recovered reason task");
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
            move || Ok(ReasonMcp::new(state.clone()))
        },
        veoveo_mcp_contract::stateless_session_manager(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(cancellation.child_token()),
    );
    let auth_state = InternalMcpAuthState { verifier };
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn(
            veoveo_mcp_contract::enforce_serialized_mcp_response,
        ))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            authenticate_internal_mcp,
        ));
    let admin_router = admin::router().layer(middleware::from_fn_with_state(
        auth_state,
        authenticate_internal_mcp,
    ));
    let service_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .with_state(state.clone())
        .nest("/admin", admin_router)
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
        service = "veoveo-reason-mcp",
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

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!ReasonMcp::tool_router().list_all().is_empty());
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
        assert_eq!(SERVER_DOCS.server(), "reason");
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
        assert_eq!(declaration.server, "reason");
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
