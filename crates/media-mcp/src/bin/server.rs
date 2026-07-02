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

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    extract::{Path as AxumPath, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    response::IntoResponse,
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, Utc};
use clap::Parser;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, CancelTaskParams, CancelTaskResult,
        CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock, CreateTaskResult,
        GetPromptRequestParams, GetPromptResult, GetTaskParams, GetTaskPayloadParams,
        GetTaskPayloadResult, GetTaskResult, JsonObject, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListTasksResult, ListToolsResult,
        PaginatedRequestParams, ProgressToken, Prompt, PromptArgument, PromptMessage,
        ReadResourceRequestParams, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, Role, ServerCapabilities, ServerInfo, SubscribeRequestParams, Task,
        TaskStatus, TasksCapability, UnsubscribeRequestParams,
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
use veoveo_mcp_contract::{
    ArtifactMetadata, ArtifactPut, GenerationPredictionSummary, GenerationRunOutput, Page,
    ProviderUris, PublicDeployment, ServerPublicEndpoint, SubscriptionHub, TaskPayloadState,
    TaskStore, UsageKind, UsageRecord, UsageReport, is_sha256, notify_progress, notify_task_status,
    now_iso, now_utc, paginate,
};
use veoveo_media_mcp::{
    artifacts::{ArtifactRepository, S3ArtifactConfig},
    provider::{BillingRecord, ModelEntry, Prediction, ProviderClient},
    state::DuckdbState,
    uris, webhook,
};

const REGISTRY_TTL: Duration = Duration::from_secs(3600);
const TASK_POLL_INTERVAL_MS: u64 = 3000;
const RUN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const BILLING_RECONCILE_INITIAL_DELAY: Duration = Duration::from_secs(10);
const BILLING_RECONCILE_MAX_DELAY: Duration = Duration::from_secs(10 * 60);
const SERVER_SLUG: &str = "media";
const LIST_PAGE_SIZE: usize = 100;

#[derive(Parser, Debug)]
#[command(name = "server", about = "Media MCP server (streamable HTTP)")]
struct Args {
    /// Port to bind on 0.0.0.0.
    #[arg(long, default_value_t = 8787)]
    port: u16,
    /// Public base URL reachable by the media provider.
    /// Required because media task completion is webhook-only.
    #[arg(long, env = "PUBLIC_BASE_URL")]
    public_base_url: String,
    /// Directory served at /media/files/* so the media provider can fetch input media by URL.
    #[arg(long)]
    static_dir: Option<PathBuf>,
    /// DuckDB state database path for task, prediction, artifact, and usage metadata.
    #[arg(long, default_value = "state.duckdb")]
    state_db: PathBuf,
    /// S3-compatible endpoint used for server-owned artifacts.
    #[arg(long, default_value = "http://localhost:9000")]
    artifact_endpoint: String,
    /// S3-compatible bucket used for server-owned artifacts.
    #[arg(long, default_value = "media-artifacts")]
    artifact_bucket: String,
    /// S3 signing region for the artifact store.
    #[arg(long, default_value = "us-east-1")]
    artifact_region: String,
    /// Allow plain HTTP for local S3-compatible artifact stores.
    #[arg(long, default_value_t = true)]
    artifact_allow_http: bool,
    #[arg(long, env = "MEDIA_PROVIDER_API_KEY", hide_env_values = true)]
    api_key: Option<String>,
    #[arg(long, env = "MEDIA_PROVIDER_WEBHOOK_SECRET", hide_env_values = true)]
    webhook_secret: Option<String>,
}

impl Args {
    fn public_deployment(&self) -> anyhow::Result<PublicDeployment> {
        PublicDeployment::new(&self.public_base_url)
    }

    fn provider_api_key(&self) -> anyhow::Result<&str> {
        self.api_key
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing MEDIA_PROVIDER_API_KEY"))
    }

    fn provider_webhook_secret(&self) -> Option<String> {
        self.webhook_secret.clone()
    }
}

struct RegistryCache {
    fetched_at: Instant,
    models: Arc<Vec<ModelEntry>>,
    by_id: HashMap<String, usize>,
}

struct AppState {
    provider: ProviderClient,
    http: reqwest::Client,
    public_endpoint: ServerPublicEndpoint,
    webhook_secret: Option<String>,
    registry: RwLock<Option<RegistryCache>>,
    tasks: TaskStore,
    durable: DuckdbState,
    artifacts: ArtifactRepository,
    /// prediction id -> waiter for its webhook callback
    pending: Mutex<HashMap<String, oneshot::Sender<Prediction>>>,
    predictions: RwLock<HashMap<String, Prediction>>,
    subscribers: SubscriptionHub,
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
            .provider
            .list_models()
            .await
            .map_err(|e| format!("failed to fetch media model registry: {e}"))?;
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
    async fn ingest_prediction(self: &Arc<Self>, prediction: Prediction) {
        let id = prediction.id.clone();
        let terminal = prediction.is_terminal();
        if let Err(e) = self.durable.record_prediction(&prediction) {
            tracing::warn!(prediction_id = id, "failed to persist prediction: {e}");
        }
        self.predictions
            .write()
            .await
            .insert(id.clone(), prediction.clone());

        if terminal {
            if let Some(tx) = self.pending.lock().await.remove(&id) {
                let _ = tx.send(prediction.clone());
            } else if let Err(e) = self.complete_task_without_peer(&prediction).await {
                tracing::warn!(
                    prediction_id = id,
                    "failed to persist webhook task completion: {e}"
                );
            }
        }

        self.subscribers
            .notify_resource_updated(uris::prediction_uri(&id))
            .await;
    }

    async fn complete_task_without_peer(
        self: &Arc<Self>,
        prediction: &Prediction,
    ) -> anyhow::Result<()> {
        let Some(task_id) = self.durable.task_id_for_provider_job_id(&prediction.id)? else {
            return Ok(());
        };
        let (status, message, payload, error) = if prediction.status == "failed" {
            let message = prediction
                .error
                .clone()
                .filter(|e| !e.is_empty())
                .unwrap_or_else(|| "prediction failed".to_string());
            (
                TaskStatus::Failed,
                format!("prediction {} failed: {message}", prediction.id),
                None,
                Some(message),
            )
        } else {
            let prediction_uri = uris::prediction_uri(&prediction.id);
            match prediction_result(self, prediction).await {
                Ok(result) => (
                    TaskStatus::Completed,
                    format!(
                        "completed; {} artifact(s); resource {prediction_uri}",
                        prediction.outputs.len()
                    ),
                    serde_json::to_value(&result).ok(),
                    None,
                ),
                Err(e) => {
                    let message = format!("artifact ingestion failed for {}: {e}", prediction.id);
                    (TaskStatus::Failed, message.clone(), None, Some(message))
                }
            }
        };
        if let Some(snapshot) = self
            .tasks
            .update(&task_id, status, message, payload.clone(), error.clone())
            .await
        {
            self.durable.record_task(
                &snapshot,
                payload.as_ref(),
                error.as_deref(),
                Some(&prediction.id),
            )?;
        }
        if prediction.status == "completed" {
            spawn_actual_usage_reconciliation(self.clone(), task_id.clone(), prediction.clone());
        }
        Ok(())
    }
}

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
    let payload_for_store = payload.clone();
    let error_for_store = error.clone();
    if let Some(snapshot) = state
        .tasks
        .update(task_id, status, message, payload, error)
        .await
    {
        if let Err(e) = state.durable.record_task(
            &snapshot,
            payload_for_store.as_ref(),
            error_for_store.as_deref(),
            None,
        ) {
            tracing::warn!(task_id, "failed to persist task update: {e}");
        }
        notify_task_status(peer, snapshot).await;
    }
}

fn usage_estimate(task_id: &str, provider_job_id: &str, entry: &ModelEntry) -> UsageRecord {
    #[derive(serde::Serialize)]
    struct EstimateUsageMetadata<'a> {
        source: &'static str,
        model_type: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        formula: Option<&'a str>,
        cost_kind: &'static str,
    }

    UsageRecord {
        task_id: task_id.to_string(),
        source_id: None,
        provider_job_id: Some(provider_job_id.to_string()),
        model_id: entry.model_id.clone(),
        kind: UsageKind::Estimate,
        quantity: Some(1.0),
        unit: Some("run".to_string()),
        amount: entry.base_price,
        currency: entry.base_price.map(|_| "USD".to_string()),
        recorded_at: now_utc(),
        metadata: serde_json::to_value(EstimateUsageMetadata {
            source: "model_registry",
            model_type: entry.model_type.as_str(),
            formula: entry.formula.as_deref(),
            cost_kind: "estimate",
        })
        .expect("estimate usage metadata serializes"),
    }
}

fn actual_usage_record(
    task_id: &str,
    prediction: &Prediction,
    billing: &BillingRecord,
) -> Option<UsageRecord> {
    #[derive(serde::Serialize)]
    struct ActualUsageMetadata<'a> {
        source: &'static str,
        billing_type: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_created_at: Option<DateTime<Utc>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_updated_at: Option<DateTime<Utc>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        order_id: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        order_state: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        order_status: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        job_status: Option<&'a str>,
    }

    let amount = billing.signed_amount()?;
    Some(UsageRecord {
        task_id: task_id.to_string(),
        source_id: Some(billing.uuid.clone()),
        provider_job_id: Some(prediction.id.clone()),
        model_id: billing
            .prediction
            .as_ref()
            .and_then(|p| p.model_uuid.clone())
            .unwrap_or_else(|| prediction.model.clone()),
        kind: UsageKind::Actual,
        quantity: Some(1.0),
        unit: Some("billing_record".to_string()),
        amount: Some(amount),
        currency: Some("USD".to_string()),
        recorded_at: now_utc(),
        metadata: serde_json::to_value(ActualUsageMetadata {
            source: "billing_record",
            billing_type: billing.billing_type.as_str(),
            source_created_at: billing.created_at,
            source_updated_at: billing.updated_at,
            order_id: billing
                .order
                .as_ref()
                .and_then(|order| order.uuid.as_deref()),
            order_state: billing
                .order
                .as_ref()
                .and_then(|order| order.state.as_deref()),
            order_status: billing
                .order
                .as_ref()
                .and_then(|order| order.status.as_deref()),
            job_status: billing
                .prediction
                .as_ref()
                .and_then(|p| p.status.as_deref()),
        })
        .expect("actual usage metadata serializes"),
    })
}

fn record_usage_estimate(
    state: &AppState,
    task_id: &str,
    provider_job_id: &str,
    entry: &ModelEntry,
) {
    let record = usage_estimate(task_id, provider_job_id, entry);
    if let Err(e) = state.durable.record_usage(&record) {
        tracing::warn!(
            task_id,
            provider_job_id,
            "failed to persist usage estimate: {e}"
        );
    }
}

async fn reconcile_actual_usage_once(
    state: &AppState,
    task_id: &str,
    prediction: &Prediction,
) -> anyhow::Result<bool> {
    if state.durable.has_actual_usage(task_id, &prediction.id)? {
        return Ok(true);
    }

    let billing_records = state.provider.billing_records(&prediction.id).await?;
    let mut recorded = 0usize;
    for billing in billing_records {
        let Some(record) = actual_usage_record(task_id, prediction, &billing) else {
            tracing::warn!(
                task_id,
                provider_job_id = prediction.id.as_str(),
                billing_id = billing.uuid,
                billing_type = billing.billing_type.as_str(),
                "provider billing row has no supported billable amount"
            );
            continue;
        };
        state.durable.record_usage(&record)?;
        recorded += 1;
    }

    if recorded > 0 {
        state
            .subscribers
            .notify_resource_updated(uris::usage_task_uri(task_id))
            .await;
    }
    Ok(recorded > 0 || state.durable.has_actual_usage(task_id, &prediction.id)?)
}

fn spawn_actual_usage_reconciliation(
    state: Arc<AppState>,
    task_id: String,
    prediction: Prediction,
) {
    tokio::spawn(async move {
        let mut delay = Duration::ZERO;
        loop {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }

            match reconcile_actual_usage_once(&state, &task_id, &prediction).await {
                Ok(true) => {
                    tracing::info!(
                        task_id,
                        provider_job_id = prediction.id.as_str(),
                        "actual usage recorded"
                    );
                    break;
                }
                Ok(false) => {
                    tracing::info!(
                        task_id,
                        provider_job_id = prediction.id.as_str(),
                        "actual usage not available yet"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        task_id,
                        provider_job_id = prediction.id.as_str(),
                        "actual usage reconciliation failed: {e}"
                    );
                }
            }

            delay = if delay.is_zero() {
                BILLING_RECONCILE_INITIAL_DELAY
            } else {
                (delay * 2).min(BILLING_RECONCILE_MAX_DELAY)
            };
        }
    });
}

async fn spawn_missing_actual_usage_reconciliations(state: Arc<AppState>) {
    let predictions: Vec<Prediction> = state
        .predictions
        .read()
        .await
        .values()
        .filter(|prediction| prediction.status == "completed")
        .cloned()
        .collect();

    for prediction in predictions {
        let task_id = match state.durable.task_id_for_provider_job_id(&prediction.id) {
            Ok(Some(task_id)) => task_id,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!(
                    provider_job_id = prediction.id,
                    "failed to find task for actual usage reconciliation: {e}"
                );
                continue;
            }
        };
        match state.durable.has_actual_usage(&task_id, &prediction.id) {
            Ok(true) => {}
            Ok(false) => spawn_actual_usage_reconciliation(state.clone(), task_id, prediction),
            Err(e) => tracing::warn!(
                task_id,
                provider_job_id = prediction.id,
                "failed to check actual usage state: {e}"
            ),
        }
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

fn filename_from_url(url: &str, index: usize) -> String {
    url.split('?')
        .next()
        .and_then(|p| p.rsplit('/').next())
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("output-{index}.bin"))
}

fn public_prediction(prediction: &Prediction) -> GenerationPredictionSummary {
    GenerationPredictionSummary {
        id: prediction.id.clone(),
        model_id: prediction.model.clone(),
        status: prediction.status.clone(),
        created_at: prediction.created_at,
        error: prediction.error.clone().filter(|error| !error.is_empty()),
        execution_ms: prediction.execution_time,
        timings: prediction.timings.clone(),
        output_count: prediction.outputs.len(),
    }
}

#[derive(serde::Serialize)]
struct OutputArtifactMetadata<'a> {
    job_id: &'a str,
    model_id: &'a str,
    output_index: usize,
}

async fn ingest_output_artifact(
    state: &AppState,
    prediction: &Prediction,
    url: &str,
    index: usize,
) -> anyhow::Result<ArtifactMetadata> {
    let response = state
        .http
        .get(url)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("provider output {index} fetch failed: {e}"))?;
    let header_mime = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = response.bytes().await?.to_vec();
    let mut artifact = ArtifactPut::new(bytes);
    artifact.mime_type = header_mime.or_else(|| guess_mime(url).map(str::to_string));
    artifact.filename = Some(filename_from_url(url, index));
    artifact.metadata = serde_json::to_value(OutputArtifactMetadata {
        job_id: prediction.id.as_str(),
        model_id: prediction.model.as_str(),
        output_index: index,
    })?;
    state.artifacts.put(artifact).await
}

async fn prediction_result(
    state: &AppState,
    prediction: &Prediction,
) -> anyhow::Result<CallToolResult> {
    let mut artifacts = Vec::new();
    for (i, url) in prediction.outputs.iter().enumerate() {
        artifacts.push(ingest_output_artifact(state, prediction, url, i).await?);
    }

    let mut blocks = vec![ContentBlock::text(format!(
        "prediction {} ({}) completed with {} artifact(s) in {:.1}s",
        prediction.id,
        prediction.model,
        artifacts.len(),
        prediction.execution_time.unwrap_or_default() / 1000.0,
    ))];
    for (i, artifact) in artifacts.iter().enumerate() {
        let mut link = Resource::new(artifact.artifact_uri.clone(), format!("output-{i}"))
            .with_description(format!("artifact {i} of prediction {}", prediction.id));
        if let Some(mime) = &artifact.mime_type {
            link = link.with_mime_type(mime.clone());
        }
        blocks.push(ContentBlock::ResourceLink(link));
    }
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(serde_json::to_value(GenerationRunOutput {
        prediction: public_prediction(prediction),
        artifacts,
    })?);
    Ok(result)
}

/// The long-running body of a `run` task: validate → submit → await webhook
/// → finalize.
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
    notify_progress(&peer, &progress_token, 1.0, "completed").await;
    let result = match prediction_result(&state, &prediction).await {
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

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ModelSelectPromptArgs {
    goal: String,
    media_type: Option<String>,
    budget: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct ImageEditPromptArgs {
    image_url: String,
    edit_goal: String,
    constraints: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct VideoPromptArgs {
    brief: String,
    reference_url: Option<String>,
    duration: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct TaskReviewPromptArgs {
    task_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MediaPrompt {
    ModelSelect,
    ImageEdit,
    VideoGenerate,
    TaskReview,
}

impl MediaPrompt {
    const ALL: [Self; 4] = [
        Self::ModelSelect,
        Self::ImageEdit,
        Self::VideoGenerate,
        Self::TaskReview,
    ];

    fn by_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|prompt| prompt.name() == name)
    }

    fn name(self) -> &'static str {
        match self {
            Self::ModelSelect => "media-model-select",
            Self::ImageEdit => "media-image-edit",
            Self::VideoGenerate => "media-video-generate",
            Self::TaskReview => "media-task-review",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::ModelSelect => "Media model selection",
            Self::ImageEdit => "Image edit request",
            Self::VideoGenerate => "Video generation request",
            Self::TaskReview => "Media task review",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::ModelSelect => "Select media models and draft valid run arguments for a goal.",
            Self::ImageEdit => "Draft an image edit request using a source image URL.",
            Self::VideoGenerate => "Draft a video generation request from a creative brief.",
            Self::TaskReview => "Review task outputs, artifacts, and usage for a completed run.",
        }
    }

    fn arguments(self) -> Vec<PromptArgument> {
        match self {
            Self::ModelSelect => vec![
                required_arg("goal", "User goal for the media generation task."),
                optional_arg(
                    "media_type",
                    "Desired output type, such as image, video, audio, or 3D.",
                ),
                optional_arg("budget", "Budget or cost guidance for model selection."),
            ],
            Self::ImageEdit => vec![
                required_arg("image_url", "Public URL of the source image."),
                required_arg("edit_goal", "Specific visual change requested by the user."),
                optional_arg(
                    "constraints",
                    "Style, brand, safety, or composition constraints.",
                ),
            ],
            Self::VideoGenerate => vec![
                required_arg("brief", "Creative brief for the video."),
                optional_arg(
                    "reference_url",
                    "Optional public image or video reference URL.",
                ),
                optional_arg("duration", "Desired duration guidance."),
            ],
            Self::TaskReview => vec![required_arg(
                "task_id",
                "MCP task id returned by the run tool.",
            )],
        }
    }

    fn prompt(self) -> Prompt {
        Prompt::new(
            self.name(),
            Some(self.description()),
            Some(self.arguments()),
        )
        .with_title(self.title())
    }

    fn render(self, arguments: Option<JsonObject>) -> Result<GetPromptResult, McpError> {
        match self {
            Self::ModelSelect => {
                let args: ModelSelectPromptArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Prepare a media model selection for this goal:\n\n\
                         Goal: {}\n\
                         Media type: {}\n\
                         Budget guidance: {}\n\n\
                         Read media://models, choose the best candidate model ids, then read \
                         media://model/{{model_id}} for each candidate before drafting run \
                         arguments. Return the selected model id and a JSON input object that \
                         conforms exactly to the selected model schema.",
                        args.goal,
                        args.media_type.as_deref().unwrap_or("not specified"),
                        args.budget.as_deref().unwrap_or("not specified"),
                    ),
                ))
            }
            Self::ImageEdit => {
                let args: ImageEditPromptArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Draft an image edit run request.\n\n\
                         Source image URL: {}\n\
                         Edit goal: {}\n\
                         Constraints: {}\n\n\
                         Read media://models and prefer an image edit or image-to-image model. \
                         Then read media://model/{{model_id}} and produce only the model id plus \
                         an input JSON object that validates against that model schema.",
                        args.image_url,
                        args.edit_goal,
                        args.constraints.as_deref().unwrap_or("not specified"),
                    ),
                ))
            }
            Self::VideoGenerate => {
                let args: VideoPromptArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Draft a video generation run request.\n\n\
                         Brief: {}\n\
                         Reference URL: {}\n\
                         Duration guidance: {}\n\n\
                         Read media://models and choose a video-capable model. Then read \
                         media://model/{{model_id}} and produce only the model id plus an input \
                         JSON object that validates against that model schema.",
                        args.brief,
                        args.reference_url.as_deref().unwrap_or("not specified"),
                        args.duration.as_deref().unwrap_or("not specified"),
                    ),
                ))
            }
            Self::TaskReview => {
                let args: TaskReviewPromptArgs = parse_prompt_args(self.name(), arguments)?;
                Ok(prompt_text(
                    self.description(),
                    format!(
                        "Review media task {}.\n\n\
                         Read tasks/get for current status. If completed, read tasks/result, \
                         inspect any media://artifact/{{sha256}} links, and read \
                         media://usage/task/{} for estimate and actual billing records. Summarize \
                         artifact count, output types, final cost, and any missing actual usage.",
                        args.task_id, args.task_id,
                    ),
                ))
            }
        }
    }
}

fn required_arg(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(true)
}

fn optional_arg(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(false)
}

fn parse_prompt_args<T: serde::de::DeserializeOwned>(
    prompt_name: &str,
    arguments: Option<JsonObject>,
) -> Result<T, McpError> {
    serde_json::from_value(Value::Object(arguments.unwrap_or_default())).map_err(|e| {
        McpError::invalid_params(
            format!("invalid arguments for prompt {prompt_name}: {e}"),
            None,
        )
    })
}

fn prompt_text(description: &str, text: String) -> GetPromptResult {
    GetPromptResult::new(vec![PromptMessage::new_text(Role::User, text)])
        .with_description(description)
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

        let progress_token = request.meta.as_ref().and_then(|m| m.get_progress_token());
        let ttl = request.task.as_ref().and_then(|t| t.ttl);
        let task_id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        let mut task = Task::new(task_id.clone(), TaskStatus::Working, now.clone(), now)
            .with_status_message("accepted; validating input")
            .with_poll_interval(TASK_POLL_INTERVAL_MS);
        task.ttl = ttl;

        self.state.tasks.insert(task.clone(), None).await;
        if let Err(e) = self.state.durable.record_task(&task, None, None, None) {
            tracing::warn!(task_id, "failed to persist task creation: {e}");
        }
        let join = tokio::spawn(run_task(
            self.state.clone(),
            context.peer.clone(),
            task_id.clone(),
            args,
            progress_token,
        ));
        self.state.tasks.set_join(&task_id, join).await;
        Ok(CreateTaskResult::new(task))
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let page = mcp_page(self.state.tasks.list().await, request.as_ref())?;
        let mut result = ListTasksResult::new(page.items);
        result.next_cursor = page.next_cursor;
        Ok(result)
    }

    async fn get_task_info(
        &self,
        request: GetTaskParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
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
        _context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
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
        _context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
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
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
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
        for (id, p) in self.state.predictions.read().await.iter() {
            resources.push(
                Resource::new(uris::prediction_uri(id), format!("prediction {id}"))
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
        } else if uri == uris::USAGE_ROOT_URI {
            let task_ids = self
                .state
                .durable
                .usage_task_ids()
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            let entries: Vec<Value> = task_ids
                .into_iter()
                .map(|task_id| {
                    json!({
                        "task_id": task_id,
                        "usage_uri": uris::usage_task_uri(&task_id),
                    })
                })
                .collect();
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
        self.state
            .subscribers
            .subscribe(request.uri.clone(), context.peer.clone())
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.state.subscribers.unsubscribe(&request.uri).await;
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
    AxumPath(sha256): AxumPath<String>,
) -> impl IntoResponse {
    if !is_sha256(&sha256) {
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
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,server=debug".into()),
        )
        .init();
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let durable = DuckdbState::open(&args.state_db)?;
    let artifacts = ArtifactRepository::new_s3_compatible(
        S3ArtifactConfig {
            endpoint: args.artifact_endpoint.clone(),
            bucket: args.artifact_bucket.clone(),
            region: args.artifact_region.clone(),
            allow_http: args.artifact_allow_http,
        },
        durable.clone(),
        ProviderUris::new("media"),
        public_endpoint.public_url().to_string(),
    )?;
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

    let state = Arc::new(AppState {
        provider: ProviderClient::new(args.provider_api_key()?),
        http: reqwest::Client::new(),
        public_endpoint: public_endpoint.clone(),
        webhook_secret: args.provider_webhook_secret(),
        registry: RwLock::new(None),
        tasks,
        durable,
        artifacts,
        pending: Mutex::new(HashMap::new()),
        predictions: RwLock::new(predictions),
        subscribers: SubscriptionHub::new(),
    });

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
    let allowed_hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
        public_deployment.host_authority().to_string(),
    ];
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(MediaMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts)
            .with_cancellation_token(ct.child_token()),
    );

    let mut server_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/webhooks", post(media_webhook))
        .route("/artifacts/{sha256}", get(artifact_download))
        .with_state(state.clone())
        .nest_service("/mcp", mcp_service);
    if let Some(dir) = &args.static_dir {
        tracing::info!(
            "serving static files from {} at {}/files",
            dir.display(),
            public_endpoint.mount_path()
        );
        server_router =
            server_router.nest_service("/files", tower_http::services::ServeDir::new(dir));
    }
    let router = Router::new().nest(public_endpoint.mount_path(), server_router);

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(
        "veoveo-media-mcp listening on http://{addr} (mcp at {}, public_url={})",
        public_endpoint.path("mcp"),
        public_endpoint.public_url()
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
