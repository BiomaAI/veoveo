//! Durable watchers for serialized deferred MCP executions.

use std::time::Duration;

use chrono::{TimeDelta, Utc};
use rig::tool::{
    DeferredInputHandler, DeferredToolDescriptor, DeferredToolResolver, DeferredToolState,
    ToolErrorKind, ToolExecutionError,
};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use veoveo_agent_runtime::{AgentRuntime, ClaimedAgentTask, json_object, wrapped_json};

use crate::{connection::ConnectionEpoch, input::DurableInputHandler, wake::WakeBus};

const TASK_CLAIM_RENEW_INTERVAL: Duration = Duration::from_secs(20);
const TASK_CLAIM_DURATION: Duration = Duration::from_secs(60);

pub fn arm_watcher(
    runtime: AgentRuntime,
    bus: WakeBus,
    epoch_rx: watch::Receiver<ConnectionEpoch>,
    task: ClaimedAgentTask,
    input_grace: Duration,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = watch_task(runtime, bus, epoch_rx, task, input_grace).await {
            tracing::error!(%error, "durable deferred-tool watcher stopped");
        }
    })
}

async fn watch_task(
    runtime: AgentRuntime,
    bus: WakeBus,
    mut epoch_rx: watch::Receiver<ConnectionEpoch>,
    task: ClaimedAgentTask,
    input_grace: Duration,
) -> anyhow::Result<()> {
    let descriptor_value =
        serde_json::Value::Object(task.descriptor.clone().into_map().into_iter().collect());
    let descriptor: DeferredToolDescriptor = match serde_json::from_value(descriptor_value) {
        Ok(descriptor) => descriptor,
        Err(error) => {
            let wake_id = runtime
                .fail_task(
                    &task,
                    wrapped_json(serde_json::json!({
                        "error": "unreadable deferred descriptor",
                        "detail": error.to_string(),
                    })),
                )
                .await?;
            bus.hint(wake_id);
            return Ok(());
        }
    };

    let epoch = epoch_rx.borrow_and_update().clone();
    let Some(resolver) = epoch.resolver else {
        retry(
            &runtime,
            &task,
            "gateway connection has no deferred resolver",
        )
        .await?;
        return Ok(());
    };
    let handle = match resolver.resolve(&descriptor).await {
        Ok(handle) => handle,
        Err(error) => {
            retry(&runtime, &task, &error.to_string()).await?;
            return Ok(());
        }
    };
    let input_handler = DurableInputHandler::new(runtime.clone(), bus.clone(), input_grace);
    let mut state = None;
    loop {
        let observed = match state.take() {
            Some(state) => state,
            None => {
                let read = handle.state();
                tokio::select! {
                    result = read => match result {
                        Ok(state) => state,
                        Err(error) => {
                            settle_error(&runtime, &bus, &task, error).await?;
                            return Ok(());
                        }
                    },
                    () = tokio::time::sleep(TASK_CLAIM_RENEW_INTERVAL) => {
                        runtime.renew_task_claim(task.agent_task_id, TASK_CLAIM_DURATION).await?;
                        continue;
                    }
                }
            }
        };
        match observed {
            DeferredToolState::Working => {
                tokio::time::sleep(Duration::from_millis(250)).await;
                runtime
                    .renew_task_claim(task.agent_task_id, TASK_CLAIM_DURATION)
                    .await?;
            }
            DeferredToolState::InputRequired(requests) => {
                let responses = input_handler.respond(&descriptor, &requests).await;
                match responses {
                    Ok(responses) => match handle.submit_input(responses).await {
                        Ok(next) => state = Some(next),
                        Err(error) => {
                            settle_error(&runtime, &bus, &task, error).await?;
                            return Ok(());
                        }
                    },
                    Err(error) => {
                        settle_error(&runtime, &bus, &task, error).await?;
                        return Ok(());
                    }
                }
            }
            DeferredToolState::Completed(result) => {
                let payload = json_object(
                    serde_json::json!({
                        "output": result.output().render(),
                        "delivered": "watcher",
                    }),
                    "deferred result",
                )?;
                let wake_id = runtime
                    .resolve_task(&task, payload, result.is_error())
                    .await?;
                bus.hint(wake_id);
                return Ok(());
            }
            DeferredToolState::Failed(error) => {
                settle_error(&runtime, &bus, &task, error).await?;
                return Ok(());
            }
            DeferredToolState::Cancelled => {
                let wake_id = runtime
                    .fail_task(
                        &task,
                        wrapped_json(serde_json::json!({ "error": "cancelled" })),
                    )
                    .await?;
                bus.hint(wake_id);
                return Ok(());
            }
            _ => {
                retry(&runtime, &task, "unsupported deferred-tool state").await?;
                return Ok(());
            }
        }
    }
}

async fn settle_error(
    runtime: &AgentRuntime,
    bus: &WakeBus,
    task: &ClaimedAgentTask,
    error: ToolExecutionError,
) -> anyhow::Result<()> {
    if matches!(
        error.kind(),
        ToolErrorKind::Network | ToolErrorKind::Timeout
    ) {
        retry(runtime, task, &error.to_string()).await?;
    } else {
        let wake_id = runtime
            .fail_task(
                task,
                wrapped_json(serde_json::json!({ "error": error.to_string() })),
            )
            .await?;
        bus.hint(wake_id);
    }
    Ok(())
}

async fn retry(runtime: &AgentRuntime, task: &ClaimedAgentTask, error: &str) -> anyhow::Result<()> {
    runtime
        .retry_task(task, Utc::now() + retry_delay(task.attempt_count), error)
        .await?;
    Ok(())
}

fn retry_delay(attempt_count: i64) -> TimeDelta {
    let exponent = u32::try_from(attempt_count.max(0))
        .unwrap_or(u32::MAX)
        .min(6);
    TimeDelta::seconds(i64::from(2u32.saturating_pow(exponent)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_backoff_caps_without_terminal_attempt_budget() {
        assert_eq!(retry_delay(0), TimeDelta::seconds(1));
        assert_eq!(retry_delay(1), TimeDelta::seconds(2));
        assert_eq!(retry_delay(100_000), TimeDelta::seconds(64));
    }
}
