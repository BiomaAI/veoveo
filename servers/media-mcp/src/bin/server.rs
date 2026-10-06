//! Media MCP server.
//!
//! One axum process exposing:
//!   /media/mcp             — MCP over streamable HTTP (rmcp)
//!   /media/webhooks/{task} — provider callback receiver (HMAC-verified)
//!   /media/files/*         — optional static media dir so providers can fetch inputs by URL
//!
//! MCP surface (protocol-maximal):
//!   tool `run(model, input)`         — durable official Tasks
//!   tool `models(query?, type?, limit?)` — catalog search for tools-only clients
//!   tool `model_schema(model)`       — exact input schema for tools-only clients
//!   tool `artifact(artifact_uri)`     — artifact image blocks for tools-only clients
//!   resource `media://models`        — compact catalog of all models
//!   template `media://model/{+model_id}`       — full input schema + pricing
//!   template `media://prediction/{id}`        — live prediction state, subscribable
//!   completion/complete over {model_id}
//!   notifications: task updates and resources/updated

use std::{
    net::SocketAddr,
    num::{NonZeroU32, NonZeroU64},
    sync::{Arc, LazyLock},
    time::Duration,
};
use veoveo_types::{TaskId, TaskTypeDefinition};

use axum::{
    Router,
    extract::{OriginalUri, Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use chrono::{TimeDelta, Utc};
use clap::Parser;
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, GetPromptRequestParams,
        GetPromptResult, Prompt, ReadResourceRequestParams, Reference, Tool,
    },
    service::RequestContext,
    tool_router,
};
use secrecy::ExposeSecret;
use tokio::sync::RwLock;
use veoveo_artifact_contract::IssueArtifactWriteCapabilityRequest;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, SubscriptionHub, TelemetryGuard,
    hosting::{
        DomainAddress, DomainRead, DomainServer, Hosted, HostedServer, completion,
        rank_completions, unknown_prompt,
    },
    init_server_telemetry,
    server_contract::McpServerSetup,
};
use veoveo_media_mcp::{
    artifacts::ArtifactRepository,
    contract::MediaGenerationResult,
    provider::{Prediction, ProviderClient},
    state::MediaState,
    uris, webhook,
};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, DurableTasks, RecoveryClass, TaskRetentionPin, TaskRuntime,
    TaskRuntimeConfig, TaskSnapshot, TaskTransition,
};

#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/artifact_tools.rs"]
mod artifact_tools;
#[path = "server/config.rs"]
mod config;
#[path = "server/generation_task.rs"]
mod generation_task;
#[path = "server/model_tools.rs"]
mod model_tools;
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
#[path = "server/subscriptions.rs"]
mod subscriptions;
#[cfg(test)]
use setup::SERVER_DOCS;
use setup::{MediaContract, SERVER_SETUP};
#[path = "server/retention.rs"]
mod retention;
#[path = "server/task_extension.rs"]
mod task_extension;
#[path = "server/usage.rs"]
mod usage;

use app_state::{
    AppState, MediaSubscriptions, spawn_provider_event_reconciliation,
    spawn_subscription_projection,
};
use config::Args;
use generation_task::submit_task;
use ownership::runtime_owner;
use prompts::MediaPrompt;
use retention::{run_retention_gc, spawn_retention_gc_loop};
use task_extension::MediaTaskExtension;
use usage::spawn_missing_actual_usage_reconciliations;
use veoveo_media_mcp::contract::ArtifactArgs;
use veoveo_media_mcp::contract::RunArgs;
use veoveo_media_mcp::contract::{ModelSchemaArgs, ModelsArgs};

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3000;
const ARTIFACT_WRITE_CAPABILITY_TTL_HOURS: i64 = 23;
const ARTIFACT_WRITE_CAPABILITY_MAX_ARTIFACTS: u32 = 64;
const ARTIFACT_WRITE_CAPABILITY_MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const BILLING_RECONCILE_INITIAL_DELAY: Duration = Duration::from_secs(10);
const BILLING_RECONCILE_MAX_DELAY: Duration = Duration::from_secs(10 * 60);
const SERVER_SLUG: &str = "media";
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);

#[derive(Clone)]
struct MediaMcp {
    state: Arc<AppState>,
    tool_router: ToolRouter<MediaMcp>,
}

#[tool_router]
impl MediaMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    /// The final task-extension middleware intercepts task-bearing calls.
    /// Direct synchronous invocation is intentionally unsupported.
    #[tool(
        title = "Run media model",
        description = "Run any media model. Run as an MCP Task and read tasks/get for status and the typed result. Find models at media://models, input schemas at media://model/{+model_id}, and billing at media://usage/task/{task_id}.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<MediaGenerationResult>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = true
        )
    )]
    async fn run(
        &self,
        Parameters(_args): Parameters<RunArgs>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`run` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "List media models",
        description = "Search the media model catalog and return exact model ids for media__run. Use this when the MCP client cannot browse media://models resources.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_media_mcp::contract::ModelCatalogOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn models(
        &self,
        Parameters(args): Parameters<ModelsArgs>,
    ) -> Result<CallToolResult, McpError> {
        let models = self
            .state
            .registry()
            .await
            .map_err(|err| McpError::internal_error(err, None))?;
        model_tools::models_result(&models, args)
    }

    #[tool(
        title = "Get media model schema",
        description = "Return the exact input JSON Schema and pricing metadata for one media model id.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_media_mcp::contract::ModelSchemaOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn model_schema(
        &self,
        Parameters(args): Parameters<ModelSchemaArgs>,
    ) -> Result<CallToolResult, McpError> {
        let model = self
            .state
            .find_model(&args.model)
            .await
            .map_err(|err| McpError::internal_error(err, None))?
            .ok_or_else(|| {
                McpError::invalid_params(
                    format!(
                        "unknown model '{}'; call models with a query or browse media://models",
                        args.model
                    ),
                    None,
                )
            })?;
        model_tools::model_schema_result(model)
    }

    #[tool(
        title = "Get media artifact",
        description = "Return a media artifact you can read as MCP image content when possible. Use this when the MCP client cannot read media://artifact/{artifact_id} resources.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<veoveo_media_mcp::contract::ArtifactOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn artifact(
        &self,
        Parameters(args): Parameters<ArtifactArgs>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        artifact_tools::artifact_result(&self.state, args, &context).await
    }
}

impl DomainServer for MediaMcp {
    type Contract = MediaContract;

    fn setup() -> &'static McpServerSetup<MediaContract> {
        &SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    fn describe_tool(&self, tool: Tool) -> Tool {
        veoveo_mcp_apps_extension::link_tool_to_app(
            tool,
            uris::STUDIO_APP_URI,
            &[
                veoveo_mcp_apps_extension::UiVisibility::Model,
                veoveo_mcp_apps_extension::UiVisibility::App,
            ],
        )
    }

    async fn read(
        &self,
        address: DomainAddress<MediaContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        self.read_media_resource(address, &request.uri, context)
            .await
            .map(DomainRead::private)
    }

    fn prompts(&self) -> Vec<Prompt> {
        MediaPrompt::ALL
            .into_iter()
            .map(MediaPrompt::prompt)
            .collect()
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        MediaPrompt::by_name(&request.name)
            .ok_or_else(|| unknown_prompt(&request.name))?
            .render(request.arguments)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        if reference.uri != uris::MODEL_TEMPLATE || request.argument.name != "model_id" {
            return Ok(CompleteResult::default());
        }
        let models = self
            .state
            .registry()
            .await
            .map_err(|e| McpError::internal_error(e, None))?;
        completion(rank_completions(
            models.iter().map(|model| model.model_id.as_str()),
            &request.argument.value,
        ))
    }
}

async fn start_media_task(
    state: Arc<AppState>,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: veoveo_mcp_contract::PlaneCaller,
    args: RunArgs,
    retention_pins: std::collections::BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let task_id = veoveo_types::TaskId::new();
    let capability = state
        .artifacts
        .issue_write_capability(
            &caller,
            &IssueArtifactWriteCapabilityRequest {
                required_data_labels: Default::default(),
                task_id: veoveo_artifact_contract::ArtifactTaskId::try_from(task_id.as_uuid())
                    .map_err(|error| error.to_string())?,
                expires_at: Utc::now() + TimeDelta::hours(ARTIFACT_WRITE_CAPABILITY_TTL_HOURS),
                max_artifact_count: NonZeroU32::new(ARTIFACT_WRITE_CAPABILITY_MAX_ARTIFACTS)
                    .expect("artifact count limit is non-zero"),
                max_total_bytes: NonZeroU64::new(ARTIFACT_WRITE_CAPABILITY_MAX_TOTAL_BYTES)
                    .expect("artifact byte limit is non-zero"),
            },
        )
        .await
        .map_err(|error| format!("issuing media task artifact capability: {error}"))?;
    let owner = runtime_owner(&identity);
    state
        .durable
        .persist_preallocated_task_context(task_id, &owner, &capability)
        .await
        .map_err(|error| format!("persisting media task write context: {error}"))?;
    state
        .tasks
        .create(DurableCreateTask {
            task_id,
            owner,
            server: SERVER_SLUG.to_owned(),
            task_type: veoveo_media_mcp::contract::MediaTaskKind::Run.name(),
            request: serde_json::to_value(&args).map_err(|error| error.to_string())?,
            recovery_class: RecoveryClass::WebhookWait,
            idempotency_key: None,
            ttl_ms: Some(state.retention.task_ttl_ms()),
            poll_interval_ms: Some(MCP_TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|error| error.to_string())?;
    let claimed = state
        .tasks
        .claim(task_id, TASK_LEASE_DURATION)
        .await
        .map_err(|error| error.to_string())?;
    let cancellation = tokio_util::sync::CancellationToken::new();
    let join = tokio::spawn({
        let state = state.clone();
        let cancellation = cancellation.clone();
        async move {
            tokio::select! {
                () = submit_task(state.clone(), task_id, args) => {}
                () = cancellation.cancelled() => {
                    if let Err(error) = state.tasks.transition(task_id, TaskTransition::Cancelled).await {
                        tracing::warn!(task_id = %task_id, "failed to persist cancelled media submission: {error}");
                    }
                }
            }
        }
    });
    state
        .tasks
        .register_worker(claimed.snapshot.task_id, cancellation, join)
        .await
        .map_err(|error| error.to_string())?;
    Ok(claimed.snapshot)
}

// ---------------------------------------------------------------------------
// Webhook + HTTP plumbing
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CallbackQuery {
    binding: String,
}

async fn media_webhook(
    State(state): State<Arc<AppState>>,
    AxumPath(task_id): AxumPath<TaskId>,
    OriginalUri(callback_uri): OriginalUri,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string()
    };
    let (id, ts, sig) = (
        header("webhook-id"),
        header("webhook-timestamp"),
        header("webhook-signature"),
    );
    if let Err(e) = webhook::verify(
        state.webhook_secret.expose_secret(),
        &id,
        &ts,
        &body,
        &sig,
        Some(300),
    ) {
        tracing::warn!("rejected webhook: {e}");
        return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
    }
    // Query admission follows provider authentication, preserving unsigned callback behavior.
    let binding = match axum::extract::Query::<CallbackQuery>::try_from_uri(&callback_uri) {
        Ok(axum::extract::Query(query)) => query.binding,
        Err(_) => return (StatusCode::UNAUTHORIZED, "invalid binding").into_response(),
    };
    let persisted =
        match veoveo_media_mcp::task_lookup::callback_digest(&state.tasks, task_id).await {
            Ok(Some(digest)) => digest,
            _ => return (StatusCode::UNAUTHORIZED, "invalid binding").into_response(),
        };
    if !webhook::CallbackBinding::verify(&persisted, &binding) {
        return (StatusCode::UNAUTHORIZED, "invalid binding").into_response();
    }
    let prediction: Prediction = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("unparseable webhook body: {e}");
            return (StatusCode::BAD_REQUEST, "bad payload").into_response();
        }
    };
    tracing::info!(
        "webhook: prediction {} -> {} ({} outputs)",
        prediction.id,
        prediction.status,
        prediction.outputs.len()
    );
    match state.receive_webhook(task_id, &id, prediction).await {
        Ok(receipt) => {
            let status = if receipt.event.processed_at.is_some() {
                StatusCode::OK
            } else {
                StatusCode::ACCEPTED
            };
            (status, "accepted").into_response()
        }
        Err(error) => {
            tracing::error!(%task_id, "failed to durably accept signed webhook: {error}");
            (StatusCode::INTERNAL_SERVER_ERROR, "durable receipt failed").into_response()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    LazyLock::force(&SERVER_SETUP);
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-media-mcp", "info,veoveo_media_mcp=debug")?;
    let args = Args::parse();
    let retention = args.retention_policy();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let artifacts = ArtifactRepository::new(args.artifact_service_url.clone());
    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            args.surreal_endpoint.clone(),
            args.surreal_namespace.clone(),
            args.surreal_database.clone(),
            args.surreal_auth_level,
            args.surreal_username.clone(),
            args.surreal_password(),
        ),
        SERVER_SLUG,
        format!("{SERVER_SLUG}-{}", uuid::Uuid::now_v7()),
    )
    .await?;
    let tasks = veoveo_media_mcp::task_lookup::bind(tasks)?;
    let recovery = tasks.recover().await?;
    if !recovery.webhook_waiting.is_empty() {
        tracing::info!(
            count = recovery.webhook_waiting.len(),
            "recovered media tasks remain waiting for signed provider webhooks"
        );
    }
    let durable = MediaState::new(tasks.platform_store().clone());

    let state = Arc::new(AppState {
        provider: ProviderClient::new(args.provider_api_key()?)
            .with_base(&args.provider_base_url)?,
        http: reqwest::Client::new(),
        public_endpoint: public_endpoint.clone(),
        webhook_secret: args.provider_webhook_secret()?,
        registry: RwLock::new(None),
        #[cfg(test)]
        registry_install_attempts: Default::default(),
        tasks,
        durable,
        artifacts,
        retention,
        subscribers: SubscriptionHub::new(),
    });

    run_retention_gc(&state).await?;
    spawn_retention_gc_loop(state.clone());
    spawn_provider_event_reconciliation(state.clone());
    spawn_missing_actual_usage_reconciliations(state.clone()).await;

    // Warm the registry so first completions/reads are instant.
    {
        let state = state.clone();
        tokio::spawn(async move {
            match state.registry().await {
                Ok(models) => tracing::info!("model registry warmed: {} models", models.len()),
                Err(e) => tracing::warn!("registry warmup failed: {e}"),
            }
        });
    }

    let mut public_routes = Router::new()
        .route("/webhooks/{task_id}", post(media_webhook))
        .with_state(state.clone());
    if let Some(dir) = &args.static_dir {
        tracing::info!(
            "serving static files from {} at {}/files",
            dir.display(),
            public_endpoint.mount_path()
        );
        public_routes =
            public_routes.nest_service("/files", tower_http::services::ServeDir::new(dir));
    }
    let readiness_store = state.tasks.platform_store().clone();
    let server = HostedServer::for_domain::<MediaMcp>()
        .deployment(&public_deployment, args.allow_loopback_hosts)?
        .allowed_hosts(args.allowed_hosts.iter().cloned())
        .readiness(move || {
            let store = readiness_store.clone();
            async move {
                matches!(
                    tokio::time::timeout(std::time::Duration::from_secs(5), store.healthcheck())
                        .await,
                    Ok(Ok(()))
                )
            }
        })
        .internal_trust(GatewayInternalTrustBundle::from_json(
            &args.internal_trust_jwks,
        )?)?
        .handler({
            let state = state.clone();
            move || {
                Hosted::new(MediaMcp::new(state.clone())).with_tasks(DurableTasks::with_resources(
                    MediaTaskExtension::new(state.clone()),
                    MediaSubscriptions::new(state.clone()),
                ))
            }
        })
        .public_routes(public_routes)
        .build();
    let _resource_observer =
        spawn_subscription_projection(state.clone(), server.cancellation_token());
    server
        .serve(SocketAddr::from(([0, 0, 0, 0], args.port)))
        .await
}

#[cfg(test)]
#[path = "server/tool_input_tests.rs"]
mod tool_input_tests;

#[cfg(test)]
mod well_known_tests {
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::SERVER_DOCS;

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "media");
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
        assert_eq!(declaration.server, "media");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        for item in &declaration.compliance {
            if item.status == ComplianceStatus::Pending {
                assert!(item.note.is_some(), "pending items must state a reason");
            }
        }
    }

    #[test]
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!MediaMcp::tool_router().list_all().is_empty());
    }

    #[test]
    fn run_tool_annotations_match_additive_open_world_behavior() {
        let tools = MediaMcp::tool_router().list_all();
        let run = tools
            .iter()
            .find(|tool| tool.name.as_ref() == "run")
            .expect("run tool should be registered");
        assert_eq!(run.title.as_deref(), Some("Run media model"));
        let annotations = run
            .annotations
            .as_ref()
            .expect("run tool should publish MCP safety annotations");
        assert_eq!(annotations.read_only_hint, Some(false));
        assert_eq!(annotations.destructive_hint, Some(false));
        assert_eq!(annotations.idempotent_hint, Some(false));
        assert_eq!(annotations.open_world_hint, Some(true));
    }
}
