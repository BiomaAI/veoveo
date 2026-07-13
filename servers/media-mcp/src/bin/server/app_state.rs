use std::{collections::HashMap, sync::Arc, time::Duration};

use rmcp::model::CallToolResult;
use secrecy::SecretString;
use serde_json::Value;
use tokio::sync::RwLock;
use veoveo_mcp_contract::{ServerPublicEndpoint, SubscriptionHub};
use veoveo_media_mcp::{
    artifacts::ArtifactRepository,
    provider::{ModelEntry, Prediction, ProviderClient},
    state::{MediaProviderEvent, MediaState, WebhookReceipt},
    uris,
};
use veoveo_platform_store::TaskStatus;
use veoveo_task_runtime::{TaskFailure, TaskRuntime};

use super::{
    config::MediaRetentionPolicy, outputs::prediction_result,
    usage::spawn_actual_usage_reconciliation,
};

const REGISTRY_TTL: Duration = Duration::from_secs(3600);
const RECONCILIATION_INTERVAL: Duration = Duration::from_millis(500);

pub(super) struct RegistryCache {
    fetched_at: std::time::Instant,
    models: Arc<Vec<ModelEntry>>,
    by_id: HashMap<String, usize>,
}

pub(super) struct AppState {
    pub(super) provider: ProviderClient,
    pub(super) http: reqwest::Client,
    pub(super) public_endpoint: ServerPublicEndpoint,
    pub(super) webhook_secret: SecretString,
    pub(super) registry: RwLock<Option<RegistryCache>>,
    pub(super) tasks: TaskRuntime,
    pub(super) durable: MediaState,
    pub(super) artifacts: ArtifactRepository,
    pub(super) retention: MediaRetentionPolicy,
    pub(super) subscribers: SubscriptionHub,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AppState")
            .field("provider", &self.provider)
            .field("webhook_secret", &"[REDACTED]")
            .finish_non_exhaustive()
    }
}

impl AppState {
    pub(super) async fn registry(&self) -> Result<Arc<Vec<ModelEntry>>, String> {
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
            .map_err(|error| format!("failed to fetch media model registry: {error}"))?;
        let models = Arc::new(models);
        let by_id = models
            .iter()
            .enumerate()
            .map(|(index, model)| (model.model_id.clone(), index))
            .collect();
        *self.registry.write().await = Some(RegistryCache {
            fetched_at: std::time::Instant::now(),
            models: models.clone(),
            by_id,
        });
        Ok(models)
    }

    pub(super) async fn find_model(&self, model_id: &str) -> Result<Option<ModelEntry>, String> {
        let models = self.registry().await?;
        let guard = self.registry.read().await;
        let Some(cache) = guard.as_ref() else {
            return Ok(None);
        };
        Ok(cache
            .by_id
            .get(model_id)
            .map(|index| models[*index].clone()))
    }

    pub(super) async fn receive_webhook(
        self: &Arc<Self>,
        task_id: &str,
        webhook_id: &str,
        prediction: Prediction,
    ) -> anyhow::Result<WebhookReceipt> {
        let receipt = self
            .durable
            .receive_webhook(&self.tasks, task_id, webhook_id, &prediction)
            .await?;
        self.subscribers
            .notify_resource_updated(uris::prediction_uri(&prediction.id))
            .await;
        if receipt.event.processed_at.is_none()
            && let Err(error) = self.process_event(&receipt.event).await
        {
            self.durable
                .record_processing_error(&receipt.event, &error.to_string())
                .await?;
            tracing::warn!(
                task_id,
                provider_job_id = prediction.id,
                "webhook is durable but completion processing will retry: {error}"
            );
        }
        Ok(receipt)
    }

    async fn process_event(self: &Arc<Self>, event: &MediaProviderEvent) -> anyhow::Result<()> {
        let task_id = event.job.task_id.to_string();
        let snapshot = self
            .tasks
            .get(&task_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("media task {task_id} no longer exists"))?;

        if matches!(
            snapshot.status,
            TaskStatus::CancelRequested | TaskStatus::Cancelled
        ) {
            self.durable
                .acknowledge_cancelled_event(&self.tasks, event)
                .await?;
            self.subscribers
                .notify_resource_updated(uris::prediction_uri(&event.prediction.id))
                .await;
            // Cancellation is a terminal local decision, not proof that the
            // provider stopped work or waived charges. The signed webhook may
            // reconcile billing, but it can never produce a task result or
            // redeem the task's artifact capability.
            spawn_actual_usage_reconciliation(self.clone(), task_id, event.prediction.clone());
            return Ok(());
        }

        if event.prediction.status == "failed" {
            let message = event
                .prediction
                .error
                .clone()
                .filter(|error| !error.is_empty())
                .unwrap_or_else(|| "provider reported media generation failure".into());
            self.durable
                .complete_event(
                    &self.tasks,
                    event,
                    Err(TaskFailure::new("provider_failed", message.clone())),
                    format!("prediction {} failed: {message}", event.prediction.id),
                )
                .await?;
            self.subscribers
                .notify_resource_updated(uris::prediction_uri(&event.prediction.id))
                .await;
            spawn_actual_usage_reconciliation(self.clone(), task_id, event.prediction.clone());
            return Ok(());
        }

        if snapshot.is_terminal() {
            self.durable
                .complete_event(
                    &self.tasks,
                    event,
                    Ok(snapshot
                        .result
                        .clone()
                        .unwrap_or(Value::Object(Default::default()))),
                    snapshot.status_message.clone().unwrap_or_default(),
                )
                .await?;
            spawn_actual_usage_reconciliation(self.clone(), task_id, event.prediction.clone());
            return Ok(());
        }
        let context =
            self.durable.task_context(&snapshot).await?.ok_or_else(|| {
                anyhow::anyhow!("media task {task_id} has no durable write context")
            })?;
        let result: CallToolResult =
            prediction_result(self, &event.prediction, &task_id, &context).await?;
        let result = serde_json::to_value(result)?;
        self.durable
            .complete_event(
                &self.tasks,
                event,
                Ok(result),
                format!(
                    "completed; {} artifact(s); resource {}",
                    event.prediction.outputs.len(),
                    uris::prediction_uri(&event.prediction.id)
                ),
            )
            .await?;
        self.subscribers
            .notify_resource_updated(uris::prediction_uri(&event.prediction.id))
            .await;
        spawn_actual_usage_reconciliation(self.clone(), task_id, event.prediction.clone());
        Ok(())
    }
}

pub(super) fn spawn_provider_event_reconciliation(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            match state.durable.pending_events(100).await {
                Ok(events) => {
                    for event in events {
                        if let Err(error) = state.process_event(&event).await {
                            let _ = state
                                .durable
                                .record_processing_error(&event, &error.to_string())
                                .await;
                            tracing::warn!(
                                webhook_id = event.webhook_id,
                                provider_job_id = event.job.external_job_id,
                                "durable media completion reconciliation failed: {error}"
                            );
                        }
                    }
                }
                Err(error) => tracing::warn!("media event reconciliation query failed: {error}"),
            }
            tokio::time::sleep(RECONCILIATION_INTERVAL).await;
        }
    });
}

/// Every replica projects committed provider outbox events into its local MCP
/// session subscriptions. Polling the outbox is reconciliation, not provider
/// status polling; SurrealDB remains the only completion source of truth.
pub(super) fn spawn_subscription_projection(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut cursor = match state.durable.store().latest_outbox_sequence().await {
            Ok(cursor) => cursor,
            Err(error) => {
                tracing::warn!("media subscription outbox baseline failed: {error}");
                0
            }
        };
        loop {
            match state.durable.store().read_outbox(cursor, 1_000).await {
                Ok(page) => {
                    cursor = page.next_sequence;
                    for event in page.events {
                        if !matches!(
                            event.aggregate_type.as_str(),
                            "provider_job" | "provider_event"
                        ) {
                            continue;
                        }
                        let payload = Value::Object(event.payload.into_map().into_iter().collect());
                        if let Some(external_job_id) =
                            payload.get("external_job_id").and_then(Value::as_str)
                        {
                            state
                                .subscribers
                                .notify_resource_updated(uris::prediction_uri(external_job_id))
                                .await;
                        }
                    }
                }
                Err(error) => tracing::warn!("media subscription outbox replay failed: {error}"),
            }
            tokio::time::sleep(RECONCILIATION_INTERVAL).await;
        }
    });
}
