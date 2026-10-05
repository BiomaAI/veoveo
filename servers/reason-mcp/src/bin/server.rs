//! Reason MCP server.
//!
//! Rerun remains the recording authority. This server resolves authorized
//! recording ranges, remuxes H.264 samples, invokes the configured world-model
//! runner, and publishes typed reasoning results plus immutable Rerun
//! annotation layers.

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
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, TelemetryGuard,
    hosting::{
        DomainAddress, DomainRead, DomainServer, Listing, gateway_identity, plane_caller,
        unknown_prompt,
    },
    init_server_telemetry,
    server_contract::McpServerSetup,
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
use veoveo_task_runtime::{DurableListener, TaskError, TaskRuntime, TaskRuntimeConfig};

#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/config.rs"]
mod config;
#[path = "server/grounding_input.rs"]
mod grounding_input;
#[path = "server/hosted.rs"]
mod hosted;
#[cfg(test)]
#[path = "server/hosted_tests/mod.rs"]
mod hosted_tests;
#[path = "server/index.rs"]
mod index;
#[path = "server/knowledge.rs"]
mod knowledge;
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
#[path = "server/subscriptions.rs"]
mod subscriptions;
#[path = "server/task_extension.rs"]
mod task_extension;
#[path = "server/task_results.rs"]
mod task_results;
#[path = "server/tasks.rs"]
mod tasks;

use app_state::AppState;
use config::Args;
use ownership::runtime_owner;
use prompts::ReasonPrompt;
use task_extension::ReasonTaskService;
use tasks::{
    ReasonTaskInput, SERVER_SLUG, TaskProgress, completed_payload, resume_task, start_reason_task,
};
use veoveo_reason_mcp::contract::{AnalysisId, ReasonResource};

use setup::ReasonContract;
#[cfg(test)]
use setup::SERVER_DOCS;

#[derive(Clone)]
struct ReasonMcp {
    state: Arc<AppState>,
    tool_router: ToolRouter<ReasonMcp>,
}

#[tool_router]
impl ReasonMcp {
    fn new(state: Arc<AppState>) -> Self {
        LazyLock::force(&setup::SERVER_SETUP);
        setup::catalog_resources(&state.catalog).expect("validated Reason catalog descriptors");
        Self {
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
            gateway_identity(&context)?,
            plane_caller(&context)?,
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

impl DomainServer for ReasonMcp {
    type Contract = ReasonContract;

    fn setup() -> &'static McpServerSetup<ReasonContract> {
        &setup::SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    fn describe_tool(&self, tool: Tool) -> Tool {
        veoveo_mcp_apps_extension::link_tool_to_app(
            tool,
            uris::ANALYSES_APP_URI,
            &[
                veoveo_mcp_apps_extension::UiVisibility::Model,
                veoveo_mcp_apps_extension::UiVisibility::App,
            ],
        )
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
        address: DomainAddress<ReasonContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        // Knowledge members follow current Artifact grants, so no read is reused.
        let knowledge = matches!(address, ReasonResource::Knowledge(_));
        let result = resources::read(&self.state, address, &request.uri, context).await?;
        Ok(if knowledge {
            DomainRead::no_store(result)
        } else {
            DomainRead::private(result)
        })
    }

    fn prompts(&self) -> Vec<Prompt> {
        ReasonPrompt::ALL
            .into_iter()
            .map(ReasonPrompt::definition)
            .collect()
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        ReasonPrompt::by_name(&request.name)
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
        if veoveo_reason_mcp::contract::FindingCollection::ALL
            .iter()
            .any(|collection| collection.member_template() == reference.uri)
            && request.argument.name == "analysis_id"
        {
            let caller = plane_caller(&context)?;
            let scope = knowledge::scope(&caller).map_err(internal)?;
            if request.argument.value.len() > 128 {
                return Err(invalid_params(
                    "finding completion prefix exceeds 128 bytes",
                ));
            }
            let mut findings = veoveo_reason_mcp::knowledge::readable_findings(
                self.state.tasks.platform_store(),
                &scope,
                veoveo_reason_mcp::knowledge::FindingSelection::Complete(&request.argument.value),
            )
            .await
            .map_err(internal)?;
            knowledge::scope(&caller).map_err(internal)?;
            let has_more = findings.len() > 100;
            findings.truncate(100);
            return Ok(CompleteResult::new(
                CompletionInfo::with_pagination(
                    findings
                        .into_iter()
                        .map(|f| f.position.analysis.to_string())
                        .collect(),
                    None,
                    has_more,
                )
                .map_err(internal)?,
            ));
        }
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
                let identity = gateway_identity(&context)?;
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
                let identity = gateway_identity(&context)?;
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

/// Reason subscriptions: analysis resources follow their tasks, and finding
/// resources follow Artifact-authorized knowledge changes.
struct ReasonListener {
    state: Arc<AppState>,
}

impl DurableListener<ReasonTaskService> for ReasonListener {
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        resources::accepted_subscription_filter(requested)
    }

    async fn listen(
        &self,
        service: &ReasonTaskService,
        context: SubscriptionContext,
    ) -> Result<(), McpError> {
        subscriptions::listen(&self.state, service, context).await
    }
}

fn invalid_params(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
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
    let tasks = veoveo_reason_mcp::task_lookup::bind(tasks)?;
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
        finding_changes: veoveo_reason_mcp::knowledge::observe::FindingChanges::new(
            tasks.platform_store().clone(),
        ),
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

    hosted::server(
        state,
        &public_deployment,
        args.allow_loopback_hosts,
        args.allowed_hosts.clone(),
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    )?
    .serve(SocketAddr::from(([0, 0, 0, 0], args.port)))
    .await
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
