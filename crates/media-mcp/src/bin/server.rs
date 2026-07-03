//! Media MCP server.
//!
//! One axum process exposing:
//!   /media/mcp             — MCP over streamable HTTP (rmcp)
//!   /media/webhooks        — internal provider callback receiver (HMAC-verified)
//!   /media/files/*         — optional static media dir so providers can fetch inputs by URL
//!   /media/artifacts/*     — immutable artifact bytes already surfaced by MCP
//!
//! MCP surface (protocol-maximal, single tool):
//!   tool `run(model, input)`         — task-required (SEP-1319)
//!   resource `media://models`        — compact catalog of all models
//!   template `media://model/{model_id}`       — full input schema + pricing
//!   template `media://prediction/{id}`        — live prediction state, subscribable
//!   completion/complete over {model_id}
//!   notifications: tasks/status, progress, resources/updated, resources/list_changed

use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    Router,
    extract::{Path as AxumPath, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    middleware,
    response::IntoResponse,
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use clap::Parser;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, CancelTaskParams, CancelTaskResult,
        CompleteRequestParams, CompleteResult, CompletionInfo, CreateTaskResult,
        GetPromptRequestParams, GetPromptResult, GetTaskParams, GetTaskPayloadParams,
        GetTaskPayloadResult, GetTaskResult, JsonObject, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListTasksResult, ListToolsResult,
        PaginatedRequestParams, ProgressToken, Prompt, ReadResourceRequestParams,
        ReadResourceResult, Reference, Resource, ResourceContents, ResourceTemplate,
        ServerCapabilities, ServerInfo, SubscribeRequestParams, Task, TaskStatus, TasksCapability,
        UnsubscribeRequestParams,
    },
    schemars,
    service::{Peer, RequestContext},
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock, oneshot};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, GenerationRunOutput,
    InternalTokenSecret, Page, ServerResourceUris, ServerSlug, SubscriptionHub, TaskPayloadState,
    TaskStore, TelemetryGuard, TokenIssuer, UsageReport, init_server_telemetry, is_sha256,
    notify_progress, now_iso, paginate, public_allowed_hosts, related_task_meta,
};
use veoveo_media_mcp::{
    artifacts::{ArtifactRepository, S3ArtifactConfig},
    provider::{ModelEntry, Prediction, ProviderClient},
    state::{DuckdbState, TaskOwner},
    uris, webhook,
};

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
#[path = "server/retention.rs"]
mod retention;
#[path = "server/usage.rs"]
mod usage;

use app_state::{AppState, update_task};
use config::{Args, ArtifactStoreBackend};
use host::validate_host;
use internal_auth::{
    InternalMcpAuthState, authenticate_internal_mcp, verify_internal_authorization,
};
use outputs::{prediction_result, public_prediction};
use ownership::{
    artifact_owned_by, artifact_owner_allows, internal_identity, optional_prediction_owner,
    optional_task_owner, prediction_owner, require_task_owner, task_owner_allows,
    task_owner_from_identity,
};
use prompts::MediaPrompt;
use retention::{run_retention_gc, spawn_retention_gc_loop};
use usage::{
    record_usage_estimate, spawn_actual_usage_reconciliation,
    spawn_missing_actual_usage_reconciliations,
};

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3000;
const RUN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const BILLING_RECONCILE_INITIAL_DELAY: Duration = Duration::from_secs(10);
const BILLING_RECONCILE_MAX_DELAY: Duration = Duration::from_secs(10 * 60);
const SERVER_SLUG: &str = "media";
const LIST_PAGE_SIZE: usize = 100;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct RunArgs {
    /// Media model id, e.g. "openai/gpt-image-2/edit". Browse the catalog
    /// at resource media://models or autocomplete via completion/complete
    /// on the media://model/{model_id} template.
    model: String,
    /// Model-specific input object. The exact JSON Schema for this model is
    /// published at resource media://model/{model_id}. Media inputs are
    /// URLs that must be reachable by the provider.
    input: JsonObject,
}

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

    /// Never executed synchronously: task_support = required routes all
    /// invocations through `enqueue_task`. This body only exists so the
    /// router publishes the tool with its schema.
    #[tool(
        description = "Run any media model asynchronously. Must be invoked as an MCP task; read tasks/get and fetch media://artifact/{sha256} outputs via tasks/result. Discover models via media://models, input schemas via media://model/{model_id}, and usage via media://usage/task/{task_id}. While running, subscribe to media://prediction/{id} (id is surfaced in the task statusMessage) for push updates.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<GenerationRunOutput>(),
        execution(task_support = "required")
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
}

/// The long-running body of a `run` task: validate → submit → await webhook
/// → finalize.
async fn run_task(
    state: Arc<AppState>,
    peer: Peer<RoleServer>,
    task_id: String,
    owner: TaskOwner,
    args: RunArgs,
    progress_token: Option<ProgressToken>,
) {
    macro_rules! fail {
        ($msg:expr) => {{
            let msg: String = $msg;
            tracing::warn!(task_id, "task failed: {msg}");
            update_task(
                &state,
                &peer,
                &task_id,
                TaskStatus::Failed,
                msg.clone(),
                None,
                Some(msg),
            )
            .await;
            return;
        }};
    }

    // 1. Resolve the model and validate input against its published schema.
    let entry = match state.find_model(&args.model).await {
        Ok(Some(entry)) => entry,
        Ok(None) => fail!(format!(
            "unknown model '{}'; browse media://models",
            args.model
        )),
        Err(e) => fail!(e),
    };
    let input = Value::Object(args.input);
    if let Some(schema) = entry.request_schema()
        && let Ok(validator) = jsonschema::validator_for(schema)
    {
        let errors: Vec<String> = validator
            .iter_errors(&input)
            .map(|e| format!("{}: {}", e.instance_path(), e))
            .collect();
        if !errors.is_empty() {
            fail!(format!(
                "input failed schema validation for {} — {}; see media://model/{}",
                args.model,
                errors.join("; "),
                args.model
            ));
        }
    }
    notify_progress(&peer, &progress_token, 0.1, "input validated").await;

    // 2. Submit with the callback URL. Completion is webhook-only.
    let webhook_url = state.public_endpoint.url("webhooks");
    let prediction = match state
        .provider
        .submit(&args.model, &input, Some(&webhook_url))
        .await
    {
        Ok(p) => p,
        Err(e) => fail!(format!("media provider submit failed: {e}")),
    };
    let prediction_id = prediction.id.clone();
    let prediction_uri = uris::prediction_uri(&prediction_id);
    state
        .predictions
        .write()
        .await
        .insert(prediction_id.clone(), prediction.clone());
    state
        .tasks
        .set_provider_job_id(&task_id, prediction_id.clone())
        .await;
    if let Err(e) = state.durable.set_provider_job_id(&task_id, &prediction_id) {
        tracing::warn!(
            task_id,
            prediction_id,
            "failed to persist provider job id: {e}"
        );
    }
    record_usage_estimate(&state, &task_id, &prediction_id, &entry);
    // A new prediction resource now exists.
    let _ = peer.notify_resource_list_changed().await;
    update_task(
        &state,
        &peer,
        &task_id,
        TaskStatus::Working,
        format!("submitted; prediction {prediction_id}; subscribe {prediction_uri} for updates"),
        None,
        None,
    )
    .await;
    notify_progress(
        &peer,
        &progress_token,
        0.3,
        &format!("submitted prediction {prediction_id}"),
    )
    .await;

    // 3. Wait for the provider webhook. No provider polling is allowed in this
    // server: a missing webhook is an operational failure.
    let (tx, mut rx) = oneshot::channel::<Prediction>();
    state.pending.lock().await.insert(prediction_id.clone(), tx);

    // A webhook may have landed between submit and waiter registration.
    let mut terminal: Option<Prediction> = state
        .predictions
        .read()
        .await
        .get(&prediction_id)
        .filter(|p| p.is_terminal())
        .cloned();
    if terminal.is_none() {
        terminal = match tokio::time::timeout(RUN_TIMEOUT, &mut rx).await {
            Ok(Ok(p)) => Some(p),
            Ok(Err(_)) => None,
            Err(_) => None,
        };
    }
    state.pending.lock().await.remove(&prediction_id);

    // 4. Finalize.
    let Some(prediction) = terminal else {
        fail!(format!(
            "timed out after {}s waiting for webhook for prediction {prediction_id}",
            RUN_TIMEOUT.as_secs()
        ));
    };
    if prediction.status == "failed" {
        let msg = prediction
            .error
            .clone()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| "prediction failed".to_string());
        fail!(format!("prediction {prediction_id} failed: {msg}"));
    }
    if state.tasks.is_terminal(&task_id).await {
        return;
    }
    notify_progress(&peer, &progress_token, 1.0, "completed").await;
    let result = match prediction_result(&state, &prediction, &task_id, &owner).await {
        Ok(result) => result,
        Err(e) => fail!(format!(
            "artifact ingestion failed for prediction {prediction_id}: {e}"
        )),
    };
    update_task(
        &state,
        &peer,
        &task_id,
        TaskStatus::Completed,
        format!(
            "completed; {} artifact(s); resource {prediction_uri}",
            prediction.outputs.len()
        ),
        serde_json::to_value(&result).ok(),
        None,
    )
    .await;
    spawn_actual_usage_reconciliation(state.clone(), task_id.clone(), prediction.clone());
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
            .enable_tasks_with(TasksCapability::server_default())
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = caps;
        info.server_info = rmcp::model::Implementation::new("media", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Async gateway to media generation models. Workflow: \
             (1) read media://models (or use completion/complete on media://model/{model_id}) to pick a model; \
             (2) optionally use prompts/list and prompts/get to draft model selection or media-specific briefs; \
             (3) read media://model/{model_id} for its exact input JSON Schema; \
             (4) call the `run` tool as a task (SEP-1319) with {model, input}; \
             (5) the task statusMessage carries the prediction id — subscribe to media://prediction/{id} for push updates; \
             (6) read tasks/get until completed, then tasks/result returns media://artifact/{sha256} links; \
             (7) read media://usage/task/{task_id} for usage estimates/actuals."
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

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        let identity = internal_identity(&context)?;
        if request.name != "run" {
            return Err(McpError::method_not_found::<
                rmcp::model::CallToolRequestMethod,
            >());
        }
        let args: RunArgs =
            serde_json::from_value(Value::Object(request.arguments.clone().unwrap_or_default()))
                .map_err(|e| {
                    McpError::invalid_params(format!("invalid run arguments: {e}"), None)
                })?;

        let progress_token = context.meta.get_progress_token();
        let ttl = request.task.as_ref().and_then(|t| t.ttl);
        let task_id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        let mut task = Task::new(task_id.clone(), TaskStatus::Working, now.clone(), now)
            .with_status_message("accepted; validating input")
            .with_poll_interval(MCP_TASK_POLL_INTERVAL_MS);
        task.ttl = ttl;

        self.state.tasks.insert(task.clone(), None).await;
        let owner = task_owner_from_identity(&task_id, &identity);
        self.state
            .durable
            .record_task_owner(&owner)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        self.state
            .task_owners
            .write()
            .await
            .insert(task_id.clone(), owner.clone());
        if let Err(e) = self.state.durable.record_task(&task, None, None, None) {
            tracing::warn!(task_id, "failed to persist task creation: {e}");
        }
        let join = tokio::spawn(run_task(
            self.state.clone(),
            context.peer.clone(),
            task_id.clone(),
            owner,
            args,
            progress_token,
        ));
        self.state.tasks.set_join(&task_id, join).await;
        Ok(CreateTaskResult::new(task).with_meta(related_task_meta(task_id)))
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let identity = internal_identity(&context)?;
        let all_tasks = self.state.tasks.list().await;
        let owners = self.state.task_owners.read().await;
        let tasks: Vec<Task> = all_tasks
            .into_iter()
            .filter(|task| {
                owners
                    .get(&task.task_id)
                    .map(|owner| task_owner_allows(owner, &identity))
                    .unwrap_or(false)
            })
            .collect();
        let page = mcp_page(tasks, request.as_ref())?;
        let mut result = ListTasksResult::new(page.items);
        result.next_cursor = page.next_cursor;
        Ok(result)
    }

    async fn get_task_info(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        require_task_owner(&self.state, &context, &request.task_id).await?;
        let task = self
            .state
            .tasks
            .get(&request.task_id)
            .await
            .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
        Ok(GetTaskResult::new(task))
    }

    async fn get_task_result(
        &self,
        request: GetTaskPayloadParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        require_task_owner(&self.state, &context, &request.task_id).await?;
        match self.state.tasks.payload_state(&request.task_id).await {
            TaskPayloadState::Completed(payload) => Ok(GetTaskPayloadResult::new(payload)),
            TaskPayloadState::Failed(error) => Err(McpError::internal_error(error, None)),
            TaskPayloadState::Cancelled => {
                Err(McpError::invalid_request("task was cancelled", None))
            }
            TaskPayloadState::Running => Err(McpError::invalid_request(
                "task is still running; read tasks/get until completed",
                None,
            )),
            TaskPayloadState::Unknown => Err(McpError::invalid_params("unknown task id", None)),
        }
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        require_task_owner(&self.state, &context, &request.task_id).await?;
        let provider_job_id = self.state.tasks.provider_job_id(&request.task_id).await;
        let task = self
            .state
            .tasks
            .cancel(&request.task_id)
            .await
            .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
        if let Some(pid) = provider_job_id {
            self.state.pending.lock().await.remove(&pid);
        }
        if let Err(e) = self.state.durable.record_task(&task, None, None, None) {
            tracing::warn!(
                task_id = request.task_id,
                "failed to persist task cancellation: {e}"
            );
        }
        Ok(CancelTaskResult::new(task))
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
        let predictions: Vec<(String, Prediction)> = self
            .state
            .predictions
            .read()
            .await
            .iter()
            .map(|(id, prediction)| (id.clone(), prediction.clone()))
            .collect();
        for (id, p) in predictions {
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
        let artifacts = self
            .state
            .durable
            .list_artifacts()
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        for artifact in artifacts {
            if !artifact_owner_allows(&self.state, &artifact.sha256, &identity)? {
                continue;
            }
            let mut resource =
                Resource::new(artifact.artifact_uri.clone(), artifact.sha256.clone())
                    .with_description(format!("artifact {}", artifact.sha256));
            if let Some(mime) = artifact.mime_type {
                resource = resource.with_mime_type(mime);
            }
            resources.push(resource);
        }
        let usage_task_ids = self
            .state
            .durable
            .usage_task_ids()
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
                .with_description("Server-owned immutable output artifact, addressed by sha256."),
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
                .predictions
                .read()
                .await
                .get(id)
                .cloned()
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
        } else if let Some(sha256) = uris::parse_artifact_uri(uri) {
            let metadata = self
                .state
                .artifacts
                .head(sha256)
                .await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown artifact '{sha256}'"), None)
                })?;
            artifact_owned_by(&self.state, &metadata.sha256, &identity)?;
            let artifact = self
                .state
                .artifacts
                .get(sha256)
                .await
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown artifact '{sha256}'"), None)
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

// ---------------------------------------------------------------------------
// Webhook + HTTP plumbing
// ---------------------------------------------------------------------------

async fn media_webhook(
    State(state): State<Arc<AppState>>,
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
    if let Some(secret) = &state.webhook_secret {
        let (id, ts, sig) = (
            header("webhook-id"),
            header("webhook-timestamp"),
            header("webhook-signature"),
        );
        if let Err(e) = webhook::verify(secret, &id, &ts, &body, &sig, Some(300)) {
            tracing::warn!("rejected webhook: {e}");
            return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
        }
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
    state.ingest_prediction(prediction).await;
    (StatusCode::OK, "ok").into_response()
}

async fn artifact_download(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AxumPath(sha256): AxumPath<String>,
) -> impl IntoResponse {
    if !is_sha256(&sha256) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let identity = match verify_internal_authorization(&state.internal_token_verifier, &headers) {
        Ok(identity) => identity,
        Err(message) => {
            tracing::warn!(
                artifact_sha256 = sha256,
                "rejected artifact download: {message}"
            );
            return (StatusCode::UNAUTHORIZED, "gateway authorization required").into_response();
        }
    };
    if let Err(err) = artifact_owned_by(&state, &sha256, &identity) {
        tracing::warn!(
            artifact_sha256 = sha256,
            "rejected artifact download: {err}"
        );
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let artifact = match state.artifacts.get(&sha256).await {
        Ok(Some(artifact)) => artifact,
        Ok(None) => return (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(e) => {
            tracing::warn!(artifact_sha256 = sha256, "artifact download failed: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "artifact unavailable").into_response();
        }
    };

    let mut headers = HeaderMap::new();
    if let Some(mime) = &artifact.metadata.mime_type
        && let Ok(value) = HeaderValue::from_str(mime)
    {
        headers.insert(CONTENT_TYPE, value);
    }
    if let Some(filename) = &artifact.metadata.filename {
        let safe = filename.replace(['"', '\r', '\n'], "_");
        if let Ok(value) = HeaderValue::from_str(&format!("inline; filename=\"{safe}\"")) {
            headers.insert(CONTENT_DISPOSITION, value);
        }
    }
    (StatusCode::OK, headers, artifact.bytes).into_response()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-media-mcp", "info,veoveo_media_mcp=debug")?;
    let args = Args::parse();
    let retention = args.retention_policy();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let durable = DuckdbState::open(&args.state_db)?;
    let internal_token_verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        InternalTokenSecret::new(args.internal_token_secret.clone())?,
    );
    let artifacts = match args.artifact_store {
        ArtifactStoreBackend::S3Compatible => ArtifactRepository::new_s3_compatible(
            S3ArtifactConfig {
                endpoint: args.artifact_endpoint.clone(),
                bucket: args.artifact_bucket.clone(),
                region: args.artifact_region.clone(),
                allow_http: args.artifact_allow_http,
            },
            durable.clone(),
            ServerResourceUris::new(SERVER_SLUG),
            public_endpoint.public_url().to_string(),
        )?,
        ArtifactStoreBackend::Memory => ArtifactRepository::new_in_memory(
            durable.clone(),
            ServerResourceUris::new(SERVER_SLUG),
            public_endpoint.public_url().to_string(),
        ),
    };
    let tasks = TaskStore::new();
    for persisted in durable.load_tasks()? {
        tasks
            .insert_record(
                persisted.task,
                persisted.payload,
                persisted.error,
                persisted.provider_job_id,
                None,
            )
            .await;
    }
    let predictions = durable
        .load_predictions()?
        .into_iter()
        .map(|p| (p.id.clone(), p))
        .collect();
    let task_owners = durable
        .load_task_owners()?
        .into_iter()
        .map(|owner| (owner.task_id.clone(), owner))
        .collect();

    let state = Arc::new(AppState {
        provider: ProviderClient::new(args.provider_api_key()?)
            .with_base(args.provider_base_url.clone()),
        http: reqwest::Client::new(),
        public_endpoint: public_endpoint.clone(),
        webhook_secret: args.provider_webhook_secret(),
        registry: RwLock::new(None),
        tasks,
        durable,
        artifacts,
        internal_token_verifier: internal_token_verifier.clone(),
        pending: Mutex::new(HashMap::new()),
        predictions: RwLock::new(predictions),
        task_owners: RwLock::new(task_owners),
        retention,
        subscribers: SubscriptionHub::new(),
    });

    run_retention_gc(&state).await?;
    spawn_retention_gc_loop(state.clone());
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
    let allowed_hosts = Arc::new(public_allowed_hosts(
        &public_deployment,
        args.allow_loopback_hosts,
    ));
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
            .with_cancellation_token(ct.child_token()),
    );
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            internal_auth_state,
            authenticate_internal_mcp,
        ));

    let mut server_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/webhooks", post(media_webhook))
        .route("/artifacts/{sha256}", get(artifact_download))
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
