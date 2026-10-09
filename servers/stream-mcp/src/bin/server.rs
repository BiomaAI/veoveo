//! Stream MCP server.
//!
//! Rerun remains the recording authority. This server resolves authorized
//! recording ranges, remuxes H.264 samples, invokes the configured DeepStream
//! runner, and publishes typed results plus immutable Rerun annotation layers.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock};

use clap::Parser;
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo,
        GetPromptRequestParams, GetPromptResult, Prompt, ReadResourceRequestParams, Reference,
        Resource, SubscriptionFilter, Tool,
    },
    service::{RequestContext, SubscriptionContext},
    tool_router,
};
use veoveo_mcp_apps_extension::{UiVisibility, link_tool_to_app};
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, SubscriptionHub, TelemetryGuard,
    hosting::{
        DomainAddress, DomainRead, DomainServer, Hosted, HostedServer, Listing, gateway_identity,
        plane_caller, unknown_prompt,
    },
    init_server_telemetry,
    server_contract::McpServerSetup,
};
use veoveo_recording_reader::RecordingReader;
use veoveo_recording_video::runtime::VideoSourceLimits;
use veoveo_stream_mcp::{
    artifacts::ArtifactRepository,
    catalog::PipelineCatalog,
    contract::{RunId, RunRecordingOutput, RunRecordingRequest},
    executor::StreamExecutor,
    uris,
};
use veoveo_task_runtime::{
    DurableListener, DurableTaskService, DurableTasks, TaskRuntime, TaskRuntimeConfig,
};

#[cfg(test)]
#[path = "server/app.rs"]
mod app;
#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/config.rs"]
mod config;
#[path = "server/index.rs"]
mod index;
#[path = "server/live.rs"]
mod live;
#[path = "server/outputs.rs"]
mod outputs;
#[path = "server/ownership.rs"]
mod ownership;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/recording_output.rs"]
mod recording_output;
#[path = "server/resources.rs"]
mod resources;
#[path = "server/setup.rs"]
mod setup;
#[cfg(test)]
#[path = "../../../../testing/fixtures/store.rs"]
mod store_fixture;
#[path = "server/subscriptions.rs"]
mod subscriptions;
#[path = "server/task_extension.rs"]
mod task_extension;
#[path = "server/task_results.rs"]
mod task_results;
#[path = "server/tasks.rs"]
mod tasks;
#[cfg(test)]
#[path = "server/test_support.rs"]
mod test_support;

use app_state::AppState;
use config::Args;
use live::LiveSessionManager;
use ownership::runtime_owner;
use prompts::StreamPrompt;
use task_extension::StreamTaskService;
use task_results::run_view;
use tasks::{SERVER_SLUG, StreamTaskInput, TaskProgress, resume_task, start_stream_task};

#[cfg(test)]
use setup::SERVER_DOCS;
use setup::StreamContract;

#[derive(Clone)]
struct StreamMcp {
    state: Arc<AppState>,
    tool_router: ToolRouter<StreamMcp>,
}

#[tool_router]
impl StreamMcp {
    fn new(state: Arc<AppState>) -> Self {
        LazyLock::force(&setup::SERVER_SETUP);
        setup::catalog_resources(&state.catalog).expect("validated Stream catalog descriptors");
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Run a pipeline over recorded video",
        description = "Run a configured Stream pipeline over a video range from a recording you can read, and publish typed results and a Rerun annotation layer.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RunRecordingOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn run_recording(
        &self,
        Parameters(request): Parameters<RunRecordingRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = gateway_identity(&context)?;
        let owner = runtime_owner(&identity);
        let snapshot = start_stream_task(
            self.state.clone(),
            identity,
            plane_caller(&context)?,
            StreamTaskInput::RunRecording(request),
            Some(TaskProgress {
                peer: context.peer.clone(),
                token: context.meta.get_progress_token(),
            }),
            BTreeSet::new(),
        )
        .await
        .map_err(internal)?;
        let task_id = RunId::try_from(snapshot.task_id).map_err(internal)?;
        task_results::completed_payload(&self.state.tasks, &owner, task_id).await
    }

    #[tool(
        title = "Start a live Stream session",
        description = "Start a configured live GStreamer pipeline. Returns right away with the RTP/H.264 ingest address and session resources you can subscribe to.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_stream_mcp::contract::StartLiveSessionOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn start_live_session(
        &self,
        Parameters(request): Parameters<veoveo_stream_mcp::contract::StartLiveSessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = runtime_owner(&gateway_identity(&context)?);
        let output = self
            .state
            .live
            .start(&request.pipeline_id, owner)
            .await
            .map_err(invalid_params)?;
        task_results::live_started_result(output).map_err(internal)
    }

    #[tool(
        title = "Stop a live Stream session",
        description = "Stop a live GStreamer session you own. Its recent results are kept.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_stream_mcp::contract::StopLiveSessionOutput>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn stop_live_session(
        &self,
        Parameters(request): Parameters<veoveo_stream_mcp::contract::StopLiveSessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = runtime_owner(&gateway_identity(&context)?);
        let output = self
            .state
            .live
            .stop(request.session_id, &owner)
            .await
            .map_err(internal)?
            .ok_or_else(|| McpError::resource_not_found("Stream session not found", None))?;
        task_results::live_stopped_result(output).map_err(internal)
    }
}

impl DomainServer for StreamMcp {
    type Contract = StreamContract;

    fn setup() -> &'static McpServerSetup<StreamContract> {
        &setup::SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    fn describe_tool(&self, tool: Tool) -> Tool {
        if matches!(
            tool.name.as_ref(),
            "start_live_session" | "stop_live_session"
        ) {
            link_tool_to_app(
                tool,
                uris::LIVE_APP_URI,
                &[UiVisibility::Model, UiVisibility::App],
            )
        } else {
            tool
        }
    }

    async fn list_resources(
        &self,
        mut declared: Vec<Resource>,
        _cursor: Option<&str>,
        _context: &RequestContext<RoleServer>,
    ) -> Result<Listing<Resource>, McpError> {
        declared.extend(setup::catalog_resources(&self.state.catalog).map_err(internal)?);
        Ok(Listing::all(declared))
    }

    async fn read(
        &self,
        address: DomainAddress<StreamContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        resources::read(&self.state, address, &request.uri, context)
            .await
            .map(DomainRead::private)
    }

    fn prompts(&self) -> Vec<Prompt> {
        StreamPrompt::ALL
            .into_iter()
            .map(StreamPrompt::definition)
            .collect()
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        StreamPrompt::by_name(&request.name)
            .ok_or_else(|| unknown_prompt(&request.name))?
            .render(request.arguments)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let needle = request.argument.value.to_ascii_lowercase();
        let values: BTreeSet<String> =
            match (reference.uri.as_str(), request.argument.name.as_str()) {
                (uris::PIPELINE_TEMPLATE, "pipelineId") => self
                    .state
                    .catalog
                    .pipeline_ids()
                    .into_iter()
                    .map(String::from)
                    .collect(),
                (uris::MODEL_TEMPLATE, "modelId") => self
                    .state
                    .catalog
                    .model_ids()
                    .into_iter()
                    .map(String::from)
                    .collect(),
                (uris::RUN_TEMPLATE | uris::RUN_RESULTS_TEMPLATE, "runId")
                | (uris::ARTIFACT_TEMPLATE, "artifact_id") => {
                    let owner = runtime_owner(&gateway_identity(&context)?);
                    let domain = if reference.uri == uris::ARTIFACT_TEMPLATE {
                        index::CompletionDomain::Artifacts
                    } else {
                        index::CompletionDomain::Runs
                    };
                    return index::complete(&self.state.tasks, &owner, domain, &needle)
                        .await
                        .map(CompleteResult::new);
                }
                (
                    uris::SESSION_TEMPLATE
                    | uris::SESSION_RESULTS_TEMPLATE
                    | uris::SESSION_PREVIEW_TEMPLATE,
                    "sessionId",
                ) => {
                    let owner = runtime_owner(&gateway_identity(&context)?);
                    let values = self
                        .state
                        .live
                        .complete_ids(&owner, &needle, CompletionInfo::MAX_VALUES + 1)
                        .await;
                    return index::bounded_completion(values).map(CompleteResult::new);
                }
                _ => return Ok(CompleteResult::default()),
            };
        let matches = values
            .into_iter()
            .filter(|value| value.contains(&needle))
            .take(CompletionInfo::MAX_VALUES + 1)
            .collect();
        index::bounded_completion(matches).map(CompleteResult::new)
    }
}

/// Stream subscriptions: run resources follow their tasks, and live-session
/// resources follow the process that owns each session.
struct StreamListener {
    state: Arc<AppState>,
}

impl DurableListener<StreamTaskService> for StreamListener {
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        resources::accepted_subscription_filter(requested)
    }

    async fn listen(
        &self,
        service: &StreamTaskService,
        context: SubscriptionContext,
    ) -> Result<(), McpError> {
        let caller = service.authenticate(context.request_context())?;
        subscriptions::listen(
            service,
            &caller,
            &self.state.tasks,
            self.state.live.clone(),
            caller.owner(),
            context,
        )
        .await
    }
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
    artifact_id: &veoveo_artifact_contract::ArtifactId,
) -> Result<veoveo_artifact_contract::ArtifactObject, McpError> {
    let metadata = state
        .artifacts
        .head(caller, artifact_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            McpError::resource_not_found(format!("Artifact `{artifact_id}` was not found."), None)
        })?;
    if metadata.byte_len > state.max_inline_resource_bytes {
        return Err(McpError::invalid_request(
            format!(
                "Artifact `{artifact_id}` is {} bytes, over the {}-byte limit for inline MCP resources. Download it through the artifact download route instead.",
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
        .ok_or_else(|| {
            McpError::resource_not_found(format!("Artifact `{artifact_id}` was not found."), None)
        })?;
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

/// Ready while the recording cache, Store and pipeline runner are.
async fn ready(state: &AppState) -> bool {
    if let Err(error) = state.recordings.readiness() {
        tracing::warn!("recording cache readiness failure: {error}");
        return false;
    }
    if let Err(error) = state.tasks.platform_store().healthcheck().await {
        tracing::warn!("Stream readiness database failure: {error}");
        return false;
    }
    if let Err(error) = state.executor.readiness() {
        tracing::warn!("Stream readiness runner failure: {error}");
        return false;
    }
    true
}

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-stream-mcp", "info,veoveo_stream_mcp=debug")?;
    let args = Args::parse();
    LazyLock::force(&setup::SERVER_SETUP);
    let live_app = veoveo_mcp_apps_extension::AppHtml::load(&args.live_app)?;
    let public_deployment = args.public_deployment()?;
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
    let tasks = veoveo_stream_mcp::task_lookup::bind(tasks)?;
    let recovery = tasks.observe_startup_recovery().await?;
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
    let executor = StreamExecutor::new(
        args.gst_runner.clone(),
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
    let subscribers = Arc::new(SubscriptionHub::new());
    let live = Arc::new(LiveSessionManager::new(
        catalog.clone(),
        args.gst_runner.clone(),
        args.live_startup_timeout(),
        args.max_live_result_frames,
        args.max_live_preview_chunks,
        args.max_detections_per_frame,
        args.max_live_event_bytes,
        args.max_live_video_chunk_bytes,
        subscribers,
    )?);
    let state = Arc::new(AppState {
        live_app,
        tasks,
        artifacts: ArtifactRepository::new(args.artifact_service_url.clone()),
        recordings,
        catalog,
        executor,
        source_limits,
        max_artifact_bytes: args.max_artifact_bytes,
        max_inline_resource_bytes: args.max_inline_resource_bytes,
        work_slots: Arc::new(tokio::sync::Semaphore::new(args.max_concurrent_jobs)),
        live,
    });
    let recovery_state = state.clone();
    let recovery = veoveo_task_runtime::TaskRecoveryObserver::start(recovery, move |report| {
        let state = recovery_state.clone();
        async move {
            for snapshot in report.resumable {
                if let Err(error) = resume_task(state.clone(), snapshot.clone()).await {
                    state
                        .tasks
                        .reconcile_recovery_claim(&snapshot, error)
                        .await?;
                }
            }
            Ok(())
        }
    })
    .await?;

    let readiness_state = state.clone();
    let server = HostedServer::for_domain::<StreamMcp>()
        .deployment(&public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .internal_trust(GatewayInternalTrustBundle::from_json(
            &args.internal_trust_jwks,
        )?)?
        .handler(move || {
            Hosted::new(StreamMcp::new(state.clone())).with_tasks(DurableTasks::with_listener(
                StreamTaskService::new(state.clone()),
                StreamListener {
                    state: state.clone(),
                },
            ))
        })
        .readiness(move || {
            let state = readiness_state.clone();
            async move { ready(&state).await }
        })
        .build();
    recovery
        .serve(server.serve(SocketAddr::from(([0, 0, 0, 0], args.port))))
        .await
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!StreamMcp::tool_router().list_all().is_empty());
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
        assert_eq!(SERVER_DOCS.server(), "stream");
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
        assert_eq!(declaration.server().as_str(), "stream");
        assert_eq!(declaration.contract_revision(), CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance()
                .iter()
                .find(|item| item.id.as_str() == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
#[cfg(test)]
#[path = "server/tool_input_tests.rs"]
mod tool_input_tests;
