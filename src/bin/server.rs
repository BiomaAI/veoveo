//! WaveSpeed MCP server.
//!
//! One axum process exposing:
//!   /mcp                 — MCP over streamable HTTP (rmcp)
//!   /webhooks/wavespeed  — WaveSpeed callback receiver (HMAC-verified)
//!   /files/*             — optional static media dir so WaveSpeed can fetch inputs by URL
//!
//! MCP surface (protocol-maximal, single tool):
//!   tool `run(model, input)`         — task-required (SEP-1319)
//!   resource `wavespeed://models`    — compact catalog of all models
//!   template `wavespeed://model/{model_id}`   — full input schema + pricing
//!   template `wavespeed://prediction/{id}`    — live prediction state, subscribable
//!   completion/complete over {model_id}
//!   notifications: tasks/status, progress, resources/updated, resources/list_changed

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use clap::Parser;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, CancelTaskParams, CancelTaskResult,
        CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock, CreateTaskResult,
        GetTaskParams, GetTaskPayloadParams, GetTaskPayloadResult, GetTaskResult, JsonObject,
        ListResourceTemplatesResult, ListResourcesResult, ListTasksResult, Notification,
        PaginatedRequestParams, ProgressNotificationParam, ProgressToken,
        ReadResourceRequestParams, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, ResourceUpdatedNotificationParam, ServerCapabilities, ServerInfo,
        ServerNotification, SubscribeRequestParams, Task, TaskStatus,
        TaskStatusNotificationParam, TasksCapability, UnsubscribeRequestParams,
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
use wavespeed_mcp::{
    uris,
    wavespeed::{ModelEntry, Prediction, WsClient},
    webhook,
};

const REGISTRY_TTL: Duration = Duration::from_secs(3600);
const TASK_POLL_INTERVAL_MS: u64 = 3000;
const FALLBACK_POLL_INTERVAL: Duration = Duration::from_secs(30);
const RUN_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Parser, Debug)]
#[command(name = "server", about = "WaveSpeed MCP server (streamable HTTP)")]
struct Args {
    /// Port to bind on 0.0.0.0.
    #[arg(long, default_value_t = 8787)]
    port: u16,
    /// Public base URL reachable by WaveSpeed (e.g. the cloudflared tunnel URL).
    /// Used to build webhook callback URLs and /files URLs. Without it the
    /// server falls back to polling WaveSpeed instead of webhooks.
    #[arg(long, env = "PUBLIC_URL")]
    public_url: Option<String>,
    /// Directory served at /files/* so WaveSpeed can fetch input media by URL.
    #[arg(long)]
    static_dir: Option<PathBuf>,
    #[arg(long, env = "WAVESPEED_API_KEY", hide_env_values = true)]
    api_key: String,
    #[arg(long, env = "WAVESPEED_WEBHOOK_SECRET", hide_env_values = true)]
    webhook_secret: Option<String>,
}

struct RegistryCache {
    fetched_at: Instant,
    models: Arc<Vec<ModelEntry>>,
    by_id: HashMap<String, usize>,
}

struct TaskEntry {
    task: Task,
    /// Serialized `CallToolResult` once terminal-completed.
    payload: Option<Value>,
    /// Error message when status == Failed.
    error: Option<String>,
    prediction_id: Option<String>,
    join: Option<tokio::task::JoinHandle<()>>,
}

struct AppState {
    ws: WsClient,
    public_url: Option<String>,
    webhook_secret: Option<String>,
    registry: RwLock<Option<RegistryCache>>,
    tasks: RwLock<HashMap<String, TaskEntry>>,
    /// prediction id -> waiter for its webhook callback
    pending: Mutex<HashMap<String, oneshot::Sender<Prediction>>>,
    predictions: RwLock<HashMap<String, Prediction>>,
    /// resource uri -> subscribed peers. We own the clients, so unsubscribe
    /// simply clears all subscriptions for the uri.
    subscribers: Mutex<HashMap<String, Vec<Peer<RoleServer>>>>,
}

impl AppState {
    async fn registry(&self) -> Result<Arc<Vec<ModelEntry>>, String> {
        {
            let guard = self.registry.read().await;
            if let Some(cache) = guard.as_ref()
                && cache.fetched_at.elapsed() < REGISTRY_TTL
            {
                return Ok(cache.models.clone());
            }
        }
        let models = self
            .ws
            .list_models()
            .await
            .map_err(|e| format!("failed to fetch wavespeed model registry: {e}"))?;
        let models = Arc::new(models);
        let by_id = models
            .iter()
            .enumerate()
            .map(|(i, m)| (m.model_id.clone(), i))
            .collect();
        *self.registry.write().await = Some(RegistryCache {
            fetched_at: Instant::now(),
            models: models.clone(),
            by_id,
        });
        Ok(models)
    }

    async fn find_model(&self, model_id: &str) -> Result<Option<ModelEntry>, String> {
        let models = self.registry().await?;
        let guard = self.registry.read().await;
        let Some(cache) = guard.as_ref() else {
            return Ok(None);
        };
        Ok(cache.by_id.get(model_id).map(|&i| models[i].clone()))
    }

    /// Record a prediction, resolve any waiter, and push resources/updated to subscribers.
    async fn ingest_prediction(&self, prediction: Prediction) {
        let id = prediction.id.clone();
        let terminal = prediction.is_terminal();
        self.predictions
            .write()
            .await
            .insert(id.clone(), prediction.clone());

        if terminal
            && let Some(tx) = self.pending.lock().await.remove(&id)
        {
            let _ = tx.send(prediction);
        }

        let uri = uris::prediction_uri(&id);
        let peers: Vec<Peer<RoleServer>> = self
            .subscribers
            .lock()
            .await
            .get(&uri)
            .cloned()
            .unwrap_or_default();
        for peer in peers {
            let _ = peer
                .notify_resource_updated(ResourceUpdatedNotificationParam::new(uri.clone()))
                .await;
        }
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct RunArgs {
    /// WaveSpeed model id, e.g. "openai/gpt-image-2/edit". Browse the catalog
    /// at resource wavespeed://models or autocomplete via completion/complete
    /// on the wavespeed://model/{model_id} template.
    model: String,
    /// Model-specific input object. The exact JSON Schema for this model is
    /// published at resource wavespeed://model/{model_id}. Media inputs are
    /// URLs that must be reachable by WaveSpeed.
    input: JsonObject,
}

#[derive(Clone)]
struct WavespeedMcp {
    state: Arc<AppState>,
    tool_router: ToolRouter<WavespeedMcp>,
}

#[tool_router]
impl WavespeedMcp {
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
        description = "Run any WaveSpeed model asynchronously. Must be invoked as an MCP task; poll tasks/get and fetch outputs via tasks/result. Discover models via the wavespeed://models resource, and each model's input schema via wavespeed://model/{model_id}. While running, subscribe to wavespeed://prediction/{id} (id is surfaced in the task statusMessage) for push updates.",
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

/// Update a task entry and push a notifications/tasks/status to the creating peer.
async fn update_task(
    state: &AppState,
    peer: &Peer<RoleServer>,
    task_id: &str,
    status: TaskStatus,
    message: impl Into<String>,
    payload: Option<Value>,
    error: Option<String>,
) {
    let snapshot = {
        let mut tasks = state.tasks.write().await;
        let Some(entry) = tasks.get_mut(task_id) else {
            return;
        };
        entry.task.status = status;
        entry.task.status_message = Some(message.into());
        entry.task.last_updated_at = now_iso();
        if payload.is_some() {
            entry.payload = payload;
        }
        if error.is_some() {
            entry.error = error;
        }
        entry.task.clone()
    };
    let _ = peer
        .send_notification(ServerNotification::TaskStatusNotification(
            Notification::new(TaskStatusNotificationParam::new(snapshot)),
        ))
        .await;
}

async fn notify_progress(
    peer: &Peer<RoleServer>,
    token: &Option<ProgressToken>,
    progress: f64,
    message: &str,
) {
    if let Some(token) = token {
        let _ = peer
            .notify_progress(
                ProgressNotificationParam::new(token.clone(), progress)
                    .with_total(1.0)
                    .with_message(message),
            )
            .await;
    }
}

fn guess_mime(url: &str) -> Option<&'static str> {
    let path = url.split('?').next()?;
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => return None,
    })
}

fn prediction_result(prediction: &Prediction) -> CallToolResult {
    let mut blocks = vec![ContentBlock::text(format!(
        "prediction {} ({}) completed with {} output(s) in {:.1}s",
        prediction.id,
        prediction.model,
        prediction.outputs.len(),
        prediction.execution_time.unwrap_or_default() / 1000.0,
    ))];
    for (i, url) in prediction.outputs.iter().enumerate() {
        let mut link = Resource::new(url.clone(), format!("output-{i}"))
            .with_description(format!("output {i} of prediction {}", prediction.id));
        if let Some(mime) = guess_mime(url) {
            link = link.with_mime_type(mime);
        }
        blocks.push(ContentBlock::ResourceLink(link));
    }
    let mut result = CallToolResult::success(blocks);
    result.structured_content = serde_json::to_value(prediction).ok();
    result
}

/// The long-running body of a `run` task: validate → submit → await webhook
/// (with poll fallback) → finalize.
async fn run_task(
    state: Arc<AppState>,
    peer: Peer<RoleServer>,
    task_id: String,
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
            "unknown model '{}'; browse wavespeed://models",
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
                "input failed schema validation for {} — {}; see wavespeed://model/{}",
                args.model,
                errors.join("; "),
                args.model
            ));
        }
    }
    notify_progress(&peer, &progress_token, 0.1, "input validated").await;

    // 2. Submit with webhook callback when we have a public URL.
    let webhook_url = state
        .public_url
        .as_ref()
        .map(|u| format!("{}/webhooks/wavespeed", u.trim_end_matches('/')));
    let prediction = match state
        .ws
        .submit(&args.model, &input, webhook_url.as_deref())
        .await
    {
        Ok(p) => p,
        Err(e) => fail!(format!("wavespeed submit failed: {e}")),
    };
    let prediction_id = prediction.id.clone();
    let prediction_uri = uris::prediction_uri(&prediction_id);
    state
        .predictions
        .write()
        .await
        .insert(prediction_id.clone(), prediction.clone());
    {
        let mut tasks = state.tasks.write().await;
        if let Some(entry) = tasks.get_mut(&task_id) {
            entry.prediction_id = Some(prediction_id.clone());
        }
    }
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

    // 3. Register the webhook waiter, then wait: webhook push wins, slow poll
    //    is the safety net for lost callbacks (or no public URL at all).
    let (tx, mut rx) = oneshot::channel::<Prediction>();
    state.pending.lock().await.insert(prediction_id.clone(), tx);

    let deadline = Instant::now() + RUN_TIMEOUT;
    let mut poll = tokio::time::interval(FALLBACK_POLL_INTERVAL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    poll.tick().await; // consume the immediate first tick

    // The webhook may have landed between submit and waiter registration.
    let mut terminal: Option<Prediction> = state
        .predictions
        .read()
        .await
        .get(&prediction_id)
        .filter(|p| p.is_terminal())
        .cloned();
    while terminal.is_none() {
        if Instant::now() > deadline {
            break;
        }
        tokio::select! {
            got = &mut rx => {
                match got {
                    Ok(p) => terminal = Some(p),
                    Err(_) => break, // sender dropped (cancel path)
                }
            }
            _ = poll.tick() => {
                match state.ws.get_prediction(&prediction_id).await {
                    Ok(p) if p.is_terminal() => {
                        state.pending.lock().await.remove(&prediction_id);
                        state.ingest_prediction(p.clone()).await;
                        terminal = Some(p);
                    }
                    Ok(p) => {
                        state.predictions.write().await.insert(prediction_id.clone(), p.clone());
                        notify_progress(&peer, &progress_token, 0.5, &format!("wavespeed status: {}", p.status)).await;
                    }
                    Err(e) => {
                        tracing::warn!("poll fallback failed for {prediction_id}: {e}");
                    }
                }
            }
        }
    }
    state.pending.lock().await.remove(&prediction_id);

    // 4. Finalize.
    let Some(prediction) = terminal else {
        fail!(format!(
            "timed out after {}s waiting for prediction {prediction_id}",
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
    notify_progress(&peer, &progress_token, 1.0, "completed").await;
    let result = prediction_result(&prediction);
    update_task(
        &state,
        &peer,
        &task_id,
        TaskStatus::Completed,
        format!(
            "completed; {} output(s); resource {prediction_uri}",
            prediction.outputs.len()
        ),
        serde_json::to_value(&result).ok(),
        None,
    )
    .await;
}

impl WavespeedMcp {
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

#[tool_handler]
impl ServerHandler for WavespeedMcp {
    fn get_info(&self) -> ServerInfo {
        let caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .enable_tasks_with(TasksCapability::server_default())
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = caps;
        info.server_info =
            rmcp::model::Implementation::new("wavespeed", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Async gateway to every WaveSpeed model. Workflow: \
             (1) read wavespeed://models (or use completion/complete on wavespeed://model/{model_id}) to pick a model; \
             (2) read wavespeed://model/{model_id} for its exact input JSON Schema; \
             (3) call the `run` tool as a task (SEP-1319) with {model, input}; \
             (4) the task statusMessage carries the prediction id — subscribe to wavespeed://prediction/{id} for push updates; \
             (5) poll tasks/get until completed, then tasks/result returns output URLs as resource links."
                .into(),
        );
        info
    }

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        if request.name != "run" {
            return Err(McpError::method_not_found::<
                rmcp::model::CallToolRequestMethod,
            >());
        }
        let args: RunArgs = serde_json::from_value(Value::Object(
            request.arguments.clone().unwrap_or_default(),
        ))
        .map_err(|e| McpError::invalid_params(format!("invalid run arguments: {e}"), None))?;

        let progress_token = request.meta.as_ref().and_then(|m| m.get_progress_token());
        let ttl = request.task.as_ref().and_then(|t| t.ttl);
        let task_id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        let mut task = Task::new(task_id.clone(), TaskStatus::Working, now.clone(), now)
            .with_status_message("accepted; validating input")
            .with_poll_interval(TASK_POLL_INTERVAL_MS);
        task.ttl = ttl;

        let join = tokio::spawn(run_task(
            self.state.clone(),
            context.peer.clone(),
            task_id.clone(),
            args,
            progress_token,
        ));
        self.state.tasks.write().await.insert(
            task_id.clone(),
            TaskEntry {
                task: task.clone(),
                payload: None,
                error: None,
                prediction_id: None,
                join: Some(join),
            },
        );
        Ok(CreateTaskResult::new(task))
    }

    async fn list_tasks(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let tasks = self.state.tasks.read().await;
        Ok(ListTasksResult::new(
            tasks.values().map(|e| e.task.clone()).collect(),
        ))
    }

    async fn get_task_info(
        &self,
        request: GetTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let tasks = self.state.tasks.read().await;
        let entry = tasks
            .get(&request.task_id)
            .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
        Ok(GetTaskResult::new(entry.task.clone()))
    }

    async fn get_task_result(
        &self,
        request: GetTaskPayloadParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        let tasks = self.state.tasks.read().await;
        let entry = tasks
            .get(&request.task_id)
            .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
        match entry.task.status {
            TaskStatus::Completed => entry
                .payload
                .clone()
                .map(GetTaskPayloadResult::new)
                .ok_or_else(|| {
                    McpError::internal_error("completed task lost its payload", None)
                }),
            TaskStatus::Failed => Err(McpError::internal_error(
                entry
                    .error
                    .clone()
                    .unwrap_or_else(|| "task failed".to_string()),
                None,
            )),
            TaskStatus::Cancelled => Err(McpError::invalid_request("task was cancelled", None)),
            _ => Err(McpError::invalid_request(
                "task is still running; poll tasks/get until completed",
                None,
            )),
        }
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        let mut tasks = self.state.tasks.write().await;
        let entry = tasks
            .get_mut(&request.task_id)
            .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
        if !matches!(
            entry.task.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            if let Some(join) = entry.join.take() {
                join.abort();
            }
            if let Some(pid) = &entry.prediction_id {
                self.state.pending.lock().await.remove(pid);
            }
            entry.task.status = TaskStatus::Cancelled;
            entry.task.status_message = Some("cancelled by client".into());
            entry.task.last_updated_at = now_iso();
        }
        Ok(CancelTaskResult::new(entry.task.clone()))
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let mut resources = vec![
            Resource::new(uris::MODELS_URI, "models")
                .with_title("WaveSpeed model catalog")
                .with_description(
                    "Compact index of every WaveSpeed model: model_id, type, description, base price.",
                )
                .with_mime_type("application/json"),
        ];
        for (id, p) in self.state.predictions.read().await.iter() {
            resources.push(
                Resource::new(uris::prediction_uri(id), format!("prediction {id}"))
                    .with_description(format!("{} — status: {}", p.model, p.status))
                    .with_mime_type("application/json"),
            );
        }
        Ok(ListResourcesResult {
            resources,
            next_cursor: None,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            resource_templates: vec![
                ResourceTemplate::new(uris::MODEL_TEMPLATE, "model")
                    .with_title("WaveSpeed model schema")
                    .with_description(
                        "Full definition of one model: input JSON Schema, pricing, description. \
                         model_id supports completion/complete.",
                    )
                    .with_mime_type("application/json"),
                ResourceTemplate::new(uris::PREDICTION_TEMPLATE, "prediction")
                    .with_title("WaveSpeed prediction state")
                    .with_description(
                        "Live state of a prediction. Subscribable: resources/updated fires when \
                         the WaveSpeed webhook reports a terminal state.",
                    )
                    .with_mime_type("application/json"),
            ],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let uri = request.uri.as_str();
        let text = if uri == uris::MODELS_URI {
            let models = self
                .state
                .registry()
                .await
                .map_err(|e| McpError::internal_error(e, None))?;
            Self::models_index_json(&models).to_string()
        } else if let Some(model_id) = uris::parse_model_uri(uri) {
            let entry = self
                .state
                .find_model(model_id)
                .await
                .map_err(|e| McpError::internal_error(e, None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(
                        format!("unknown model '{model_id}'; browse wavespeed://models"),
                        None,
                    )
                })?;
            serde_json::to_string(&entry)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        } else if let Some(id) = uris::parse_prediction_uri(uri) {
            // Serve the cache when terminal; otherwise fetch fresh state.
            let cached = self.state.predictions.read().await.get(id).cloned();
            let prediction = match cached {
                Some(p) if p.is_terminal() => p,
                cached => match self.state.ws.get_prediction(id).await {
                    Ok(p) => {
                        self.state.ingest_prediction(p.clone()).await;
                        p
                    }
                    Err(e) => cached.ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("unknown prediction '{id}': {e}"),
                            None,
                        )
                    })?,
                },
            };
            serde_json::to_string(&prediction)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
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
        self.state
            .subscribers
            .lock()
            .await
            .entry(request.uri.clone())
            .or_default()
            .push(context.peer.clone());
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.state.subscribers.lock().await.remove(&request.uri);
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

async fn wavespeed_webhook(
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,server=debug".into()),
        )
        .init();
    let args = Args::parse();

    let state = Arc::new(AppState {
        ws: WsClient::new(&args.api_key),
        public_url: args.public_url.clone(),
        webhook_secret: args.webhook_secret.clone(),
        registry: RwLock::new(None),
        tasks: RwLock::new(HashMap::new()),
        pending: Mutex::new(HashMap::new()),
        predictions: RwLock::new(HashMap::new()),
        subscribers: Mutex::new(HashMap::new()),
    });

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
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(WavespeedMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default().with_cancellation_token(ct.child_token()),
    );

    let mut router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/webhooks/wavespeed", post(wavespeed_webhook))
        .with_state(state.clone())
        .nest_service("/mcp", mcp_service);
    if let Some(dir) = &args.static_dir {
        tracing::info!("serving static files from {} at /files", dir.display());
        router = router.nest_service("/files", tower_http::services::ServeDir::new(dir));
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(
        "wavespeed-mcp listening on http://{addr} (mcp at /mcp, public_url={:?})",
        args.public_url
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
