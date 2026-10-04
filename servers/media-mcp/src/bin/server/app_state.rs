use std::{collections::HashMap, sync::Arc, time::Duration};
use veoveo_types::TaskId;

use rmcp::{ErrorData as McpError, RoleServer, model::CallToolResult, service::RequestContext};
use secrecy::SecretString;
use serde_json::Value;
use tokio::sync::RwLock;
use veoveo_mcp_contract::{
    ServerPublicEndpoint, SubscriptionHub,
    hosting::{ResourceSubscriptions, gateway_identity},
};
use veoveo_media_mcp::{
    artifacts::ArtifactRepository,
    contract::{MediaModelId, ModelEntry},
    contract::{MediaPredictionUri, MediaResource},
    provider::{Prediction, ProviderClient},
    state::{MediaProviderEvent, MediaState, WebhookReceipt},
    task_results::GENERATION_COMPLETED,
};
use veoveo_platform_store::TaskStatus;
use veoveo_task_runtime::{TaskFailure, TaskRuntime};

use super::{
    config::MediaRetentionPolicy, outputs::prediction_result, ownership::runtime_owner,
    subscriptions, usage::spawn_actual_usage_reconciliation,
};

const REGISTRY_TTL: Duration = Duration::from_secs(3600);
const RECONCILIATION_INTERVAL: Duration = Duration::from_millis(500);

pub(super) struct RegistryCache {
    fetched_at: std::time::Instant,
    models: Arc<Vec<ModelEntry>>,
    by_id: HashMap<MediaModelId, usize>,
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

    pub(super) async fn find_model(
        &self,
        model_id: &MediaModelId,
    ) -> Result<Option<ModelEntry>, String> {
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
        task_id: TaskId,
        webhook_id: &str,
        prediction: Prediction,
    ) -> anyhow::Result<WebhookReceipt> {
        let receipt = self
            .durable
            .receive_webhook(&self.tasks, task_id, webhook_id, &prediction)
            .await?;
        self.subscribers
            .notify_resource_updated(MediaPredictionUri::new(prediction.id.clone()).to_string())
            .await;
        if receipt.event.processed_at.is_none()
            && let Err(error) = self.process_event(&receipt.event).await
        {
            self.durable
                .record_processing_error(&receipt.event, &error.to_string())
                .await?;
            tracing::warn!(
                %task_id,
                provider_job_id = %prediction.id,
                "webhook is durable but completion processing will retry: {error}"
            );
        }
        Ok(receipt)
    }

    async fn process_event(self: &Arc<Self>, event: &MediaProviderEvent) -> anyhow::Result<()> {
        let task_id = event.job.task_id;
        let snapshot = self
            .tasks
            .get(task_id)
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
                .notify_resource_updated(
                    MediaPredictionUri::new(event.prediction.id.clone()).to_string(),
                )
                .await;
            // Cancellation is a terminal local decision, not proof that the
            // provider stopped work or waived charges. The signed webhook may
            // reconcile billing, but it can never produce a task result or
            // redeem the task's artifact capability.
            spawn_actual_usage_reconciliation(
                self.clone(),
                event.job.task_id,
                event.prediction.clone(),
            );
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
                .notify_resource_updated(
                    MediaPredictionUri::new(event.prediction.id.clone()).to_string(),
                )
                .await;
            spawn_actual_usage_reconciliation(
                self.clone(),
                event.job.task_id,
                event.prediction.clone(),
            );
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
            spawn_actual_usage_reconciliation(
                self.clone(),
                event.job.task_id,
                event.prediction.clone(),
            );
            return Ok(());
        }
        let context =
            self.durable.task_context(&snapshot).await?.ok_or_else(|| {
                anyhow::anyhow!("media task {task_id} has no durable write context")
            })?;
        let result: CallToolResult =
            prediction_result(self, &event.prediction, event.job.task_id, &context).await?;
        let result = serde_json::to_value(result)?;
        self.durable
            .complete_event(&self.tasks, event, Ok(result), GENERATION_COMPLETED.into())
            .await?;
        self.subscribers
            .notify_resource_updated(
                MediaPredictionUri::new(event.prediction.id.clone()).to_string(),
            )
            .await;
        spawn_actual_usage_reconciliation(
            self.clone(),
            event.job.task_id,
            event.prediction.clone(),
        );
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
                                provider_job_id = %event.job.external_job_id,
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

/// Every replica observes committed prediction and billing changes through one
/// shared Store LIVE source. Resource reads still enforce the caller's authority.
/// Media resource changes, admitted for the subscribing caller's task authority.
pub(super) struct MediaSubscriptions {
    state: Arc<AppState>,
}

impl MediaSubscriptions {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl ResourceSubscriptions for MediaSubscriptions {
    type Address = MediaResource;

    async fn authorize(
        &self,
        addresses: Vec<MediaResource>,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let owner = runtime_owner(&gateway_identity(context)?);
        subscriptions::authorize(&self.state.tasks, &owner, addresses).await
    }

    fn hub(&self) -> &SubscriptionHub {
        &self.state.subscribers
    }
}

pub(super) fn spawn_subscription_projection(
    state: Arc<AppState>,
    cancellation: tokio_util::sync::CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use futures::StreamExt;
        use veoveo_platform_store::PlatformTable::{ProviderJob, Task};
        let mut changes = state.durable.store().resource_changes(vec![
            veoveo_modules::ObservationTable::from(ProviderJob),
            veoveo_media_mcp::MediaObservationTable::MediaUsage.into(),
            Task.into(),
        ]);
        loop {
            tokio::select! {
                () = cancellation.cancelled() => break,
                change = changes.next() => {
                    if change.is_none() { break; }
                    state.subscribers.notify_resource_contents_changed().await;
                }
            }
        }
    })
}
