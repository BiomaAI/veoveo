use std::sync::Arc;

use rmcp::{model::JsonObject, schemars};
use serde_json::Value;
use veoveo_task_runtime::{TaskFailure, TaskTransition};

use super::{AppState, usage::record_usage_estimate};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub(super) struct RunArgs {
    /// Media model id. Browse media://models or complete the model template.
    pub(super) model: String,
    /// Model-specific input matching media://model/{model_id}.
    pub(super) input: JsonObject,
}

/// Validate and submit one provider job. The worker intentionally stops after
/// the durable provider binding enters `waiting`; only a signed webhook can
/// drive the terminal transition.
pub(super) async fn submit_task(state: Arc<AppState>, task_id: String, args: RunArgs) {
    let entry = match state.find_model(&args.model).await {
        Ok(Some(entry)) => entry,
        Ok(None) => {
            fail(
                &state,
                &task_id,
                "unknown_model",
                format!("unknown model '{}'; browse media://models", args.model),
            )
            .await;
            return;
        }
        Err(error) => {
            fail(&state, &task_id, "model_registry_failed", error).await;
            return;
        }
    };
    let input = Value::Object(args.input);
    if let Some(schema) = entry.request_schema()
        && let Ok(validator) = jsonschema::validator_for(schema)
    {
        let errors: Vec<String> = validator
            .iter_errors(&input)
            .map(|error| format!("{}: {error}", error.instance_path()))
            .collect();
        if !errors.is_empty() {
            fail(
                &state,
                &task_id,
                "invalid_model_input",
                format!(
                    "input failed schema validation for {}: {}; see media://model/{}",
                    args.model,
                    errors.join("; "),
                    args.model
                ),
            )
            .await;
            return;
        }
    }
    if let Err(error) = state
        .tasks
        .transition(
            &task_id,
            TaskTransition::Running {
                message: "input validated; submitting provider job".into(),
                progress: 0.1,
            },
        )
        .await
    {
        tracing::warn!(
            task_id,
            "failed to publish media validation progress: {error}"
        );
        return;
    }

    let webhook_url = state.public_endpoint.url(&format!("webhooks/{task_id}"));
    let prediction = match state
        .provider
        .submit(&args.model, &input, Some(&webhook_url))
        .await
    {
        Ok(prediction) => prediction,
        Err(error) => {
            fail(
                &state,
                &task_id,
                "provider_submit_failed",
                format!("media provider submission failed: {error}"),
            )
            .await;
            return;
        }
    };

    match state
        .durable
        .bind_submission_and_wait(&state.tasks, &task_id, &prediction)
        .await
    {
        Ok(job) => {
            if let Err(error) = record_usage_estimate(&state, &task_id, &job, &entry).await {
                tracing::warn!(task_id, "failed to persist usage estimate: {error}");
            }
            tracing::info!(
                task_id,
                provider_job_id = prediction.id,
                "media task is durably waiting for a signed webhook"
            );
        }
        Err(error) => {
            // The provider may already be running. Do not submit again and do
            // not query it. The task remains webhook-recoverable through its
            // task-specific callback URL.
            tracing::error!(
                task_id,
                provider_job_id = prediction.id,
                "provider accepted the job but durable binding failed: {error}"
            );
        }
    }
}

async fn fail(state: &AppState, task_id: &str, code: &str, message: String) {
    tracing::warn!(task_id, "media submission failed: {message}");
    if let Err(error) = state
        .tasks
        .transition(
            task_id,
            TaskTransition::Failed(TaskFailure::new(code, message)),
        )
        .await
    {
        tracing::warn!(task_id, "failed to persist media task failure: {error}");
    }
}
