use std::{sync::Arc, time::Duration};

use rmcp::{
    RoleServer,
    model::{JsonObject, ProgressToken, TaskStatus},
    schemars,
    service::Peer,
};
use serde_json::Value;
use tokio::sync::oneshot;
use veoveo_mcp_contract::notify_progress;
use veoveo_media_mcp::{provider::Prediction, state::TaskOwner, uris};

use super::{
    app_state::{AppState, update_task},
    outputs::prediction_result,
    usage::{record_usage_estimate, spawn_actual_usage_reconciliation},
};

const RUN_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub(super) struct RunArgs {
    /// Media model id, e.g. "openai/gpt-image-2/edit". Browse the catalog
    /// at resource media://models or autocomplete via completion/complete
    /// on the media://model/{model_id} template.
    pub(super) model: String,
    /// Model-specific input object. The exact JSON Schema for this model is
    /// published at resource media://model/{model_id}. Media inputs are
    /// URLs that must be reachable by the provider.
    pub(super) input: JsonObject,
}

/// The long-running body of a `run` task: validate, submit, await webhook,
/// and finalize.
pub(super) async fn run_task(
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

    // Provider completion is webhook-only. This server never polls provider
    // status for task completion.
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
