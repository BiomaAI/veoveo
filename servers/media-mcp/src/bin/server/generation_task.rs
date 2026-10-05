use std::sync::Arc;
use veoveo_types::TaskId;

use serde_json::Value;
use veoveo_task_runtime::{TaskFailure, TaskTransition};

use super::{AppState, usage::record_usage_estimate};

use veoveo_media_mcp::contract::RunArgs;

/// Validate and submit one provider job. The worker intentionally stops after
/// the durable provider binding enters `waiting`; only a signed webhook can
/// drive the terminal transition.
pub(super) async fn submit_task(state: Arc<AppState>, task_id: TaskId, args: RunArgs) {
    let entry = match state.find_model(&args.model).await {
        Ok(Some(entry)) => entry,
        Ok(None) => {
            fail(
                &state,
                task_id,
                "unknown_model",
                format!("unknown model '{}'; browse media://models", args.model),
            )
            .await;
            return;
        }
        Err(error) => {
            fail(&state, task_id, "model_registry_failed", error).await;
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
                task_id,
                "invalid_model_input",
                format!(
                    "input failed schema validation for {}: {}; see {}",
                    args.model,
                    errors.join("; "),
                    veoveo_media_mcp::contract::MediaModelUri::new(args.model.clone())
                ),
            )
            .await;
            return;
        }
    }
    if let Err(error) = state
        .tasks
        .transition(
            task_id,
            TaskTransition::Running {
                message: "input validated; submitting provider job".into(),
                progress: 0.1,
            },
        )
        .await
    {
        tracing::warn!(
            %task_id,
            "failed to publish media validation progress: {error}"
        );
        return;
    }

    let preparation = async {
        let current = state
            .tasks
            .get(task_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("media task disappeared before dispatch"))?;
        let context = state
            .durable
            .task_context(&current)
            .await?
            .ok_or_else(|| anyhow::anyhow!("media task has no private dispatch context"))?;
        let provider = veoveo_types::ExtensionName::parse("media")?;
        let binding = veoveo_media_mcp::webhook::CallbackBinding::derive(
            &context.artifact_write_capability.secret,
            task_id,
            &current.owner.authority.tenant,
            &provider,
        );
        let dispatch = veoveo_media_mcp::task_lookup::dispatch(&current, binding.digest())?;
        let prepared = state
            .tasks
            .webhooks(provider)
            .prepare_dispatch(task_id, dispatch)
            .await?;
        if prepared == veoveo_task_runtime::DispatchPreparation::AlreadyPrepared {
            return Ok::<_, anyhow::Error>(None);
        }
        let mut url =
            reqwest::Url::parse(&state.public_endpoint.url(&format!("webhooks/{task_id}")))
                .map_err(|_| anyhow::anyhow!("invalid configured media callback endpoint"))?;
        url.query_pairs_mut()
            .append_pair("binding", binding.expose_secret());
        Ok(Some(url))
    }
    .await;
    let webhook_url = match preparation {
        Ok(Some(url)) => url,
        Ok(None) => return,
        Err(error) => {
            tracing::warn!(%task_id, "media dispatch preparation failed: {error}");
            return;
        }
    };
    let prediction = match state
        .provider
        .submit(&args.model, &input, Some(webhook_url.as_str()))
        .await
    {
        Ok(prediction) => prediction,
        Err(error) => {
            // A lost response cannot prove whether the provider accepted the dispatch.
            // The committed receipt forbids another send and keeps webhook recovery pinned.
            tracing::warn!(%task_id, "media submission outcome remains unresolved: {error}");
            return;
        }
    };

    match state
        .durable
        .bind_submission_and_wait(&state.tasks, task_id, &prediction)
        .await
    {
        Ok(job) => {
            if let Err(error) = record_usage_estimate(&state, job.task_id, &job, &entry).await {
                tracing::warn!(%task_id, "failed to persist usage estimate: {error}");
            }
            state.subscribers.notify_resource_contents_changed().await;
            tracing::info!(
                %task_id,
                provider_job_id = %prediction.id,
                "media task is durably waiting for a signed webhook"
            );
        }
        Err(error) => {
            // The provider may already be running. Do not submit again and do
            // not query it. The task remains webhook-recoverable through its
            // task-specific callback URL.
            tracing::error!(
                %task_id,
                provider_job_id = %prediction.id,
                "provider accepted the job but durable binding failed: {error}"
            );
        }
    }
}

async fn fail(state: &AppState, task_id: TaskId, code: &str, message: String) {
    tracing::warn!(%task_id, "media submission failed: {message}");
    if let Err(error) = state
        .tasks
        .transition(
            task_id,
            TaskTransition::Failed(TaskFailure::new(code, message)),
        )
        .await
    {
        tracing::warn!(%task_id, "failed to persist media task failure: {error}");
    }
}
