use std::sync::Arc;
use veoveo_types::TaskId;

use serde_json::Value;
use veoveo_task_runtime::{
    RecoveryClass, TaskError, TaskFailure, TaskRuntime, TaskSnapshot, TaskStatus, TaskTransition,
};

use super::{AppState, usage::record_usage_estimate};

use veoveo_media_mcp::contract::RunArgs;

/// Validate and submit one provider job. The worker intentionally stops after
/// the durable provider binding enters `waiting`; only a signed webhook can
/// drive the terminal transition.
pub(super) async fn submit_task(state: Arc<AppState>, task_id: TaskId, args: RunArgs) {
    let admitted = match state.tasks.get(task_id).await {
        Ok(Some(snapshot)) => snapshot,
        _ => {
            tracing::warn!(%task_id, "media dispatch has no admitted current Task");
            return;
        }
    };
    let entry = match state.find_model(&args.model).await {
        Ok(Some(entry)) => entry,
        Ok(None) => {
            fail(
                &state,
                &admitted,
                "unknown_model",
                format!("unknown model '{}'; browse media://models", args.model),
            )
            .await;
            return;
        }
        Err(error) => {
            fail(&state, &admitted, "model_registry_failed", error).await;
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
                &admitted,
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
    match settle_before_dispatch(
        &state.tasks,
        &admitted,
        TaskTransition::Running {
            message: "input validated; submitting provider job".into(),
            progress: 0.1,
        },
    )
    .await
    {
        Ok(current) if current.status == TaskStatus::Running => (),
        Ok(_) => return,
        Err(error) => {
            tracing::warn!(%task_id, %error, "media validation checkpoint remains unresolved");
            return;
        }
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
            // Only current cancellation with proven absence of receipt/association can settle.
            // An uncertain prepare transaction may have committed; never authorize another send.
            if let Err(settlement) =
                settle_before_dispatch(&state.tasks, &admitted, TaskTransition::Cancelled).await
            {
                tracing::warn!(%task_id, %settlement, "media pre-dispatch cancellation remains unresolved");
            }
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

async fn fail(state: &AppState, admitted: &TaskSnapshot, code: &str, message: String) {
    let task_id = admitted.task_id;
    tracing::warn!(%task_id, "media submission failed: {message}");
    if let Err(error) = settle_before_dispatch(
        &state.tasks,
        admitted,
        TaskTransition::Failed(TaskFailure::new(code, message)),
    )
    .await
    {
        tracing::warn!(%task_id, %error, "media pre-dispatch failure remains unresolved");
    }
}

/// This path ends before provider dispatch. Receipt or read uncertainty forbids settlement.
pub(super) async fn settle_before_dispatch(
    tasks: &TaskRuntime,
    admitted: &TaskSnapshot,
    next: TaskTransition,
) -> Result<TaskSnapshot, TaskError> {
    let current = tasks
        .get(admitted.task_id)
        .await?
        .ok_or_else(|| TaskError::NotFound(admitted.task_id.to_string()))?;
    settle_before_dispatch_if_current(tasks, admitted, &current, next).await
}

pub(super) async fn settle_before_dispatch_if_current(
    tasks: &TaskRuntime,
    admitted: &TaskSnapshot,
    selected: &TaskSnapshot,
    next: TaskTransition,
) -> Result<TaskSnapshot, TaskError> {
    let error = match settle_selected(tasks, admitted, selected, next.clone()).await {
        Ok(snapshot) => return Ok(snapshot),
        Err(error) => error,
    };
    if !matches!(
        error,
        TaskError::Conflict(_) | TaskError::InvalidTransition { .. }
    ) {
        return Err(error);
    }
    let Some(current) = tasks.get(admitted.task_id).await? else {
        return Err(error);
    };
    if current.status != TaskStatus::CancelRequested {
        return Err(error);
    }
    settle_selected(tasks, admitted, &current, next).await
}

async fn settle_selected(
    tasks: &TaskRuntime,
    admitted: &TaskSnapshot,
    current: &TaskSnapshot,
    next: TaskTransition,
) -> Result<TaskSnapshot, TaskError> {
    if current.task_id != admitted.task_id
        || current.owner != admitted.owner
        || current.server != admitted.server
        || current.task_type != admitted.task_type
        || current.request != admitted.request
        || current.created_at != admitted.created_at
        || current.recovery_class != admitted.recovery_class
        || current.recovery_class != RecoveryClass::WebhookWait
        || current.server != tasks.server()
    {
        return Err(TaskError::InvalidRecord(
            "Media pre-dispatch Task identity changed".into(),
        ));
    }
    if current.is_terminal() {
        return Ok(current.clone());
    }
    if current.lease_owner.as_deref() != Some(tasks.worker_id())
        || current
            .lease_expires_at
            .is_none_or(|expiry| expiry <= chrono::Utc::now())
    {
        return Err(TaskError::LeaseHeld(current.task_id.to_string()));
    }
    let provider = veoveo_types::ExtensionName::parse("media")
        .map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
    if veoveo_media_mcp::task_lookup::callback_digest(tasks, current.task_id)
        .await?
        .is_some()
        || tasks
            .webhooks(provider)
            .job_for_task(current.task_id)
            .await?
            .is_some()
    {
        return Err(TaskError::InvalidRecord(
            "Media dispatch receipt requires webhook reconciliation".into(),
        ));
    }
    let next = if current.status == TaskStatus::CancelRequested {
        TaskTransition::Cancelled
    } else if matches!(next, TaskTransition::Cancelled) {
        return Err(TaskError::InvalidTransition {
            from: current.status,
            to: TaskStatus::Cancelled,
        });
    } else {
        next
    };
    // New dispatch cannot commit from CancelRequested. A competing prepare updates
    // the Task version and clears its lease, so this selected CAS fails closed.
    tasks.transition_if_current(current, next).await
}
