//! Media MCP server.
//!
//! One axum process exposing:
//!   /media/mcp             — MCP over streamable HTTP (rmcp)
//!   /media/webhooks/{task} — provider callback receiver (HMAC-verified)
//!   /media/files/*         — optional static media dir so providers can fetch inputs by URL
//!
//! MCP surface (protocol-maximal):
//!   tool `run(model, input)`         — durable final task extension
//!   tool `models(query?, type?, limit?)` — catalog search for tools-only clients
//!   tool `model_schema(model)`       — exact input schema for tools-only clients
//!   tool `artifact(artifact_uri)`     — artifact image blocks for tools-only clients
//!   resource `media://models`        — compact catalog of all models
//!   template `media://model/{model_id}`       — full input schema + pricing
//!   template `media://prediction/{id}`        — live prediction state, subscribable
//!   completion/complete over {model_id}
//!   notifications: task updates and resources/updated

use std::{
    net::SocketAddr,
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode},
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{TimeDelta, Utc};
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
use secrecy::ExposeSecret;
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GatewayInternalTrustBundle,
    GenerationRunOutput, IssueArtifactWriteCapabilityRequest, Page, ServerSlug, SubscriptionHub,
    TelemetryGuard, TokenIssuer, UsageReport, init_server_telemetry, paginate,
    public_allowed_hosts,
};
use veoveo_mcp_task_extension::{
    Implementation as TaskExtensionImplementation, ServerDiscovery, TaskExtensionAdapter,
    task_extension_middleware,
};
use veoveo_media_mcp::{
    artifacts::ArtifactRepository,
    provider::{ModelEntry, Prediction, ProviderClient},
    state::MediaState,
    uris, webhook,
};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, RecoveryClass, TaskRetentionPin, TaskRuntime,
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
#[path = "server/host.rs"]
mod host;
#[path = "server/internal_auth.rs"]
mod internal_auth;
#[path = "server/model_tools.rs"]
mod model_tools;
#[path = "server/outputs.rs"]
mod outputs;
#[path = "server/ownership.rs"]
mod ownership;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/retention.rs"]
mod retention;
#[path = "server/task_extension.rs"]
mod task_extension;
#[path = "server/usage.rs"]
mod usage;

use app_state::{AppState, spawn_provider_event_reconciliation, spawn_subscription_projection};
use artifact_tools::ArtifactArgs;
use config::Args;
use generation_task::{RunArgs, submit_task};
use host::validate_host;
use internal_auth::{InternalMcpAuthState, authenticate_internal_mcp};
use model_tools::{ModelSchemaArgs, ModelsArgs};
use outputs::public_prediction;
use ownership::{
    internal_caller, internal_identity, optional_prediction_owner, optional_task_owner,
    prediction_owner, require_task_owner, runtime_owner, task_owner_allows,
};
use prompts::MediaPrompt;
use retention::{run_retention_gc, spawn_retention_gc_loop};
use task_extension::MediaTaskExtension;
use usage::spawn_missing_actual_usage_reconciliations;

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
const LIST_PAGE_SIZE: usize = 100;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);

#[derive(Clone)]
struct MediaMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
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
        description = "Run any media model as a durable asynchronous task. Read tasks/get for status and the terminal typed result. Discover models through media://models, schemas through media://model/{model_id}, and billing through media://usage/task/{task_id}.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<GenerationRunOutput>(),
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
            "run requires task-based invocation",
            None,
        ))
    }

    #[tool(
        title = "List media models",
        description = "Search the media model catalog and return exact model ids for media__run. Use this when the MCP client cannot browse media://models resources.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<model_tools::ModelCatalogOutput>(),
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
        output_schema = rmcp::handler::server::tool::schema_for_type::<model_tools::ModelSchemaOutput>(),
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
        description = "Return an authorized media artifact as MCP image content when possible. Use this when the MCP client cannot read media://artifact/{artifact_id} resources.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<artifact_tools::ArtifactOutput>(),
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

impl MediaMcp {
    fn models_index_json(models: &[ModelEntry]) -> Value {
        Value::Array(
            models
                .iter()
                .map(|m| {
                    json!({
                        "model_id": m.model_id,
                        "type": m.model_type,
                        "description": m.description,
                        "base_price": m.base_price,
                        "schema_uri": uris::model_uri(&m.model_id),
                    })
                })
                .collect(),
        )
    }
}

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE)
        .map_err(|e| McpError::invalid_params(e.to_string(), None))
}

#[tool_handler]
impl ServerHandler for MediaMcp {
    fn get_info(&self) -> ServerInfo {
        let caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = caps;
        info.server_info = rmcp::model::Implementation::new("media", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Async gateway to media generation models. Workflow: \
             (1) read media://models (or use completion/complete on media://model/{model_id}) to pick a model; \
             (2) optionally use prompts/list and prompts/get to draft model selection or media-specific briefs; \
             (3) read media://model/{model_id} for its exact input JSON Schema; \
             (4) call the `run` tool through the negotiated task extension with {model, input}; \
             (5) read tasks/get or subscribe through subscriptions/listen; \
             (6) the completed task result contains media://artifact/{artifact_id} links; \
             (7) read media://usage/task/{task_id} for usage estimates and actual billing."
                .into(),
        );
        info
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        let page = mcp_page(tools, request.as_ref())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts: Vec<Prompt> = MediaPrompt::ALL
            .into_iter()
            .map(MediaPrompt::prompt)
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
        let prompt = MediaPrompt::by_name(&request.name).ok_or_else(|| {
            McpError::invalid_params(
                format!("unknown prompt '{}'; read prompts/list", request.name),
                None,
            )
        })?;
        prompt.render(request.arguments)
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let identity = internal_identity(&context)?;
        let mut resources = vec![
            Resource::new(uris::MODELS_URI, "models")
                .with_title("Media model catalog")
                .with_description(
                    "Compact index of every media model: model_id, type, description, base price.",
                )
                .with_mime_type("application/json"),
            Resource::new(uris::USAGE_ROOT_URI, "usage")
                .with_title("Media usage ledger")
                .with_description("Index of task usage resources.")
                .with_mime_type("application/json"),
        ];
        let predictions = self
            .state
            .durable
            .provider_jobs()
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        for job in predictions {
            let id = job.external_job_id;
            let p = job.prediction;
            let Some(owner) = optional_prediction_owner(&self.state, &id).await? else {
                continue;
            };
            if !task_owner_allows(&owner, &identity) {
                continue;
            }
            resources.push(
                Resource::new(uris::prediction_uri(&id), format!("prediction {id}"))
                    .with_description(format!("{} — status: {}", p.model, p.status))
                    .with_mime_type("application/json"),
            );
        }
        // Artifacts live on the shared plane now; media keeps no local artifact
        // index to enumerate. They remain readable by their media://artifact URI
        // through resources/read, which resolves against the plane.
        let usage_task_ids = self
            .state
            .durable
            .usage_task_ids()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        for task_id in usage_task_ids {
            let Some(owner) = optional_task_owner(&self.state, &task_id).await? else {
                continue;
            };
            if !task_owner_allows(&owner, &identity) {
                continue;
            }
            resources.push(
                Resource::new(
                    uris::usage_task_uri(&task_id),
                    format!("usage for task {task_id}"),
                )
                .with_description("Usage estimates and actuals for one task.")
                .with_mime_type("application/json"),
            );
        }
        resources.sort_by(|a, b| a.uri.cmp(&b.uri));
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
            ResourceTemplate::new(uris::MODEL_TEMPLATE, "model")
                .with_title("Media model schema")
                .with_description(
                    "Full definition of one model: input JSON Schema, pricing, description. \
                         model_id supports completion/complete.",
                )
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::PREDICTION_TEMPLATE, "prediction")
                .with_title("Media prediction state")
                .with_description(
                    "Live state of a prediction. Subscribable: resources/updated fires when \
                         the provider reports a terminal state.",
                )
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
                .with_title("Media artifact")
                .with_description(
                    "Server-owned immutable output artifact, addressed by occurrence id.",
                ),
            ResourceTemplate::new(uris::USAGE_TASK_TEMPLATE, "usage")
                .with_title("Media task usage")
                .with_description("Usage estimates and actuals for one task, addressed by task id.")
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
        let identity = internal_identity(&context)?;
        let uri = request.uri.as_str();
        let text = if uri == uris::MODELS_URI {
            let models = self
                .state
                .registry()
                .await
                .map_err(|e| McpError::internal_error(e, None))?;
            Self::models_index_json(&models).to_string()
        } else if uri == uris::USAGE_ROOT_URI {
            let task_ids = self
                .state
                .durable
                .usage_task_ids()
                .await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            let mut entries: Vec<Value> = Vec::new();
            for task_id in task_ids {
                let Some(owner) = optional_task_owner(&self.state, &task_id).await? else {
                    continue;
                };
                if !task_owner_allows(&owner, &identity) {
                    continue;
                }
                entries.push(json!({
                    "task_id": task_id,
                    "usage_uri": uris::usage_task_uri(&task_id),
                }));
            }
            serde_json::to_string(&entries)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        } else if let Some(model_id) = uris::parse_model_uri(uri) {
            let entry = self
                .state
                .find_model(model_id)
                .await
                .map_err(|e| McpError::internal_error(e, None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(
                        format!("unknown model '{model_id}'; browse media://models"),
                        None,
                    )
                })?;
            serde_json::to_string(&entry)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        } else if let Some(id) = uris::parse_prediction_uri(uri) {
            let owner = prediction_owner(&self.state, id).await?;
            if !task_owner_allows(&owner, &identity) {
                return Err(McpError::invalid_request(
                    "media prediction policy denied request",
                    None,
                ));
            }
            let prediction = self
                .state
                .durable
                .provider_job_for_external(id)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .map(|job| job.prediction)
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown prediction '{id}'"), None)
                })?;
            serde_json::to_string(&public_prediction(&prediction))
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        } else if let Some(task_id) = uris::parse_usage_task_uri(uri) {
            require_task_owner(&self.state, &context, task_id).await?;
            let records = self
                .state
                .durable
                .usage_records(task_id)
                .await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            if records.is_empty() {
                return Err(McpError::resource_not_found(
                    format!("unknown usage task '{task_id}'"),
                    None,
                ));
            }
            let report = UsageReport::new(task_id, uri).with_records(records);
            serde_json::to_string(&report)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        } else if let Some(artifact_id) = uris::parse_artifact_uri(uri) {
            // The plane enforces access with the caller's identity.
            let caller = internal_caller(&context)?;
            let artifact = self
                .state
                .artifacts
                .get(&caller, &artifact_id)
                .await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown artifact '{artifact_id}'"), None)
                })?;
            let blob = BASE64_STANDARD.encode(&artifact.bytes);
            let mut content = ResourceContents::blob(blob, uri);
            if let Some(mime) = artifact.metadata.mime_type {
                content = content.with_mime_type(mime);
            }
            return Ok(ReadResourceResult::new(vec![content]));
        } else {
            return Err(McpError::resource_not_found(
                format!("unknown resource uri: {uri}"),
                None,
            ));
        };
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, uri).with_mime_type("application/json"),
        ]))
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = internal_identity(&context)?;
        let prediction_id = uris::parse_prediction_uri(&request.uri)
            .ok_or_else(|| McpError::invalid_params("resource is not subscribable", None))?;
        let owner = prediction_owner(&self.state, prediction_id).await?;
        if !task_owner_allows(&owner, &identity) {
            return Err(McpError::invalid_request(
                "media subscription policy denied request",
                None,
            ));
        }
        self.state
            .subscribers
            .subscribe(
                request.uri.clone(),
                identity.principal.id.clone(),
                context.peer.clone(),
            )
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = internal_identity(&context)?;
        let prediction_id = uris::parse_prediction_uri(&request.uri)
            .ok_or_else(|| McpError::invalid_params("resource is not subscribable", None))?;
        let owner = prediction_owner(&self.state, prediction_id).await?;
        if !task_owner_allows(&owner, &identity) {
            return Err(McpError::invalid_request(
                "media subscription policy denied request",
                None,
            ));
        }
        self.state
            .subscribers
            .unsubscribe(&request.uri, &identity.principal.id)
            .await;
        Ok(())
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(res_ref) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        if res_ref.uri != uris::MODEL_TEMPLATE || request.argument.name != "model_id" {
            return Ok(CompleteResult::default());
        }
        let needle = request.argument.value.to_lowercase();
        let models = self
            .state
            .registry()
            .await
            .map_err(|e| McpError::internal_error(e, None))?;
        // Prefix matches rank above substring matches.
        let mut prefixed: Vec<&str> = Vec::new();
        let mut contained: Vec<&str> = Vec::new();
        for m in models.iter() {
            let id = m.model_id.to_lowercase();
            if id.starts_with(&needle) {
                prefixed.push(&m.model_id);
            } else if id.contains(&needle) {
                contained.push(&m.model_id);
            }
        }
        let total = (prefixed.len() + contained.len()) as u32;
        let values: Vec<String> = prefixed
            .into_iter()
            .chain(contained)
            .take(CompletionInfo::MAX_VALUES)
            .map(String::from)
            .collect();
        let has_more = (values.len() as u32) < total;
        let completion = CompletionInfo::with_pagination(values, Some(total), has_more)
            .map_err(|e| McpError::internal_error(e, None))?;
        Ok(CompleteResult::new(completion))
    }
}

async fn start_media_task(
    state: Arc<AppState>,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: veoveo_mcp_contract::PlaneCaller,
    args: RunArgs,
    retention_pins: std::collections::BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let task_id = veoveo_task_runtime::TaskId::new();
    let task_id_text = task_id.to_string();
    let capability = state
        .artifacts
        .issue_write_capability(
            &caller,
            &IssueArtifactWriteCapabilityRequest {
                task_id: task_id_text.clone(),
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
            task_type: "run".to_owned(),
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
        .claim(&task_id_text, TASK_LEASE_DURATION)
        .await
        .map_err(|error| error.to_string())?;
    let cancellation = tokio_util::sync::CancellationToken::new();
    let join = tokio::spawn({
        let state = state.clone();
        let cancellation = cancellation.clone();
        async move {
            tokio::select! {
                () = submit_task(state.clone(), task_id_text.clone(), args) => {}
                () = cancellation.cancelled() => {
                    if let Err(error) = state.tasks.transition(&task_id_text, TaskTransition::Cancelled).await {
                        tracing::warn!(task_id = task_id_text, "failed to persist cancelled media submission: {error}");
                    }
                }
            }
        }
    });
    state
        .tasks
        .register_worker(&claimed.snapshot.task_id.to_string(), cancellation, join)
        .await
        .map_err(|error| error.to_string())?;
    Ok(claimed.snapshot)
}

// ---------------------------------------------------------------------------
// Webhook + HTTP plumbing
// ---------------------------------------------------------------------------

async fn media_webhook(
    State(state): State<Arc<AppState>>,
    AxumPath(task_id): AxumPath<String>,
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
    match state.receive_webhook(&task_id, &id, prediction).await {
        Ok(receipt) => {
            let status = if receipt.event.processed_at.is_some() {
                StatusCode::OK
            } else {
                StatusCode::ACCEPTED
            };
            (status, "accepted").into_response()
        }
        Err(error) => {
            tracing::error!(task_id, "failed to durably accept signed webhook: {error}");
            (StatusCode::INTERNAL_SERVER_ERROR, "durable receipt failed").into_response()
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-media-mcp", "info,veoveo_media_mcp=debug")?;
    let args = Args::parse();
    let retention = args.retention_policy();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let internal_token_verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
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
            .with_base(args.provider_base_url.clone()),
        http: reqwest::Client::new(),
        public_endpoint: public_endpoint.clone(),
        webhook_secret: args.provider_webhook_secret()?,
        registry: RwLock::new(None),
        tasks,
        durable,
        artifacts,
        retention,
        subscribers: SubscriptionHub::new(),
    });

    run_retention_gc(&state).await?;
    spawn_retention_gc_loop(state.clone());
    spawn_provider_event_reconciliation(state.clone());
    spawn_subscription_projection(state.clone());
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

    let ct = tokio_util::sync::CancellationToken::new();
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts);
    let internal_auth_state = InternalMcpAuthState {
        verifier: internal_token_verifier,
    };
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(MediaMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_stateful_mode(true)
            .with_json_response(true)
            .with_cancellation_token(ct.child_token()),
    );
    let task_extension = Arc::new(TaskExtensionAdapter::new(
        Arc::new(MediaTaskExtension::new(state.clone())),
        ServerDiscovery::new(
            std::collections::BTreeMap::from([
                ("tools".to_owned(), json!({})),
                ("resources".to_owned(), json!({})),
            ]),
            TaskExtensionImplementation {
                name: "media".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some(
                "Webhook-completed media generation with durable tasks and immutable artifacts."
                    .to_owned(),
            ),
        ),
    ));
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            task_extension,
            task_extension_middleware::<MediaTaskExtension>,
        ))
        .layer(middleware::from_fn_with_state(
            internal_auth_state,
            authenticate_internal_mcp,
        ));

    let mut server_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/webhooks/{task_id}", post(media_webhook))
        .with_state(state.clone())
        .nest("/mcp", mcp_router);
    if let Some(dir) = &args.static_dir {
        tracing::info!(
            "serving static files from {} at {}/files",
            dir.display(),
            public_endpoint.mount_path()
        );
        server_router =
            server_router.nest_service("/files", tower_http::services::ServeDir::new(dir));
    }
    let router = Router::new()
        .nest(public_endpoint.mount_path(), server_router)
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(
        service = "veoveo-media-mcp",
        address = %addr,
        mcp_path = public_endpoint.path("mcp"),
        public_url = public_endpoint.public_url(),
        "listening"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
