use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use rmcp::{RoleServer, model::TaskStatus, service::Peer};
use serde_json::Value;
use tokio::sync::{Mutex, RwLock, oneshot};
use veoveo_mcp_contract::{
    GatewayInternalTokenIssuer, GatewayInternalTokenVerifier, ServerPublicEndpoint,
    SubscriptionHub, TaskStore, TokenIssuer,
};
use veoveo_media_mcp::{
    artifacts::ArtifactRepository,
    provider::{ModelEntry, Prediction, ProviderClient},
    state::{DuckdbState, TaskOwner},
    uris,
};

use super::{
    config::MediaRetentionPolicy, outputs::prediction_result, ownership::task_owner,
    usage::spawn_actual_usage_reconciliation,
};

const REGISTRY_TTL: Duration = Duration::from_secs(3600);

pub(super) struct RegistryCache {
    fetched_at: Instant,
    models: Arc<Vec<ModelEntry>>,
    by_id: HashMap<String, usize>,
}

pub(super) struct AppState {
    pub(super) provider: ProviderClient,
    pub(super) http: reqwest::Client,
    pub(super) public_endpoint: ServerPublicEndpoint,
    pub(super) webhook_secret: Option<String>,
    pub(super) registry: RwLock<Option<RegistryCache>>,
    pub(super) tasks: TaskStore,
    pub(super) durable: DuckdbState,
    pub(super) artifacts: ArtifactRepository,
    pub(super) internal_token_verifier: GatewayInternalTokenVerifier,
    /// Mints short-lived internal tokens for async artifact writes that complete
    /// under a persisted TaskOwner (the provider webhook path), where no live
    /// gateway bearer exists. See `ownership::plane_caller_for_owner`.
    pub(super) internal_token_issuer: GatewayInternalTokenIssuer,
    /// Issuer name stamped into those minted tokens.
    pub(super) internal_token_issuer_name: TokenIssuer,
    /// prediction id -> waiter for its webhook callback
    pub(super) pending: Mutex<HashMap<String, oneshot::Sender<Prediction>>>,
    pub(super) predictions: RwLock<HashMap<String, Prediction>>,
    pub(super) task_owners: RwLock<HashMap<String, TaskOwner>>,
    pub(super) retention: MediaRetentionPolicy,
    pub(super) subscribers: SubscriptionHub,
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

    pub(super) async fn find_model(&self, model_id: &str) -> Result<Option<ModelEntry>, String> {
        let models = self.registry().await?;
        let guard = self.registry.read().await;
        let Some(cache) = guard.as_ref() else {
            return Ok(None);
        };
        Ok(cache.by_id.get(model_id).map(|&i| models[i].clone()))
    }

    /// Record a prediction, resolve any waiter, and push resources/updated to subscribers.
    pub(super) async fn ingest_prediction(self: &Arc<Self>, prediction: Prediction) {
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
        if self.tasks.is_terminal(&task_id).await {
            return Ok(());
        }
        let owner = task_owner(self, &task_id)
            .await
            .map_err(|err| anyhow::anyhow!("task ownership lookup failed: {err}"))?;
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
            match prediction_result(self, prediction, &task_id, &owner).await {
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

pub(super) async fn update_task(
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
        veoveo_mcp_contract::notify_task_status(peer, snapshot).await;
    }
}
