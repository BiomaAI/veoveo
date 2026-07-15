//! Durable watchers for detached MCP tasks.
//!
//! Every attempt owns a SurrealDB lease. Transient transport failures persist
//! a retry schedule and release that lease; retry count is diagnostic only and
//! never expires a task. A terminal result and its wake commit atomically.

use std::time::Duration;

use chrono::{TimeDelta, Utc};
use rig_core::tool::{TaskResumer, ToolErrorKind, ToolTaskDescriptor, ToolTaskStatus};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use veoveo_agent_runtime::{AgentRuntime, ClaimedAgentTask, json_object, wrapped_json};

use crate::{connection::ConnectionEpoch, wake::WakeBus};

const TASK_CLAIM_RENEW_INTERVAL: Duration = Duration::from_secs(20);
const TASK_CLAIM_DURATION: Duration = Duration::from_secs(60);

pub fn arm_watcher(
    runtime: AgentRuntime,
    bus: WakeBus,
    epoch_rx: watch::Receiver<ConnectionEpoch>,
    task: ClaimedAgentTask,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(error) = watch_task(runtime, bus, epoch_rx, task).await {
            tracing::error!(%error, "durable task watcher stopped");
        }
    })
}

async fn watch_task(
    runtime: AgentRuntime,
    bus: WakeBus,
    mut epoch_rx: watch::Receiver<ConnectionEpoch>,
    task: ClaimedAgentTask,
) -> anyhow::Result<()> {
    let descriptor_value =
        serde_json::Value::Object(task.descriptor.clone().into_map().into_iter().collect());
    let descriptor: ToolTaskDescriptor = match serde_json::from_value(descriptor_value) {
        Ok(descriptor) => descriptor,
        Err(error) => {
            let wake_id = runtime
                .fail_task(
                    &task,
                    wrapped_json(serde_json::json!({
                        "error": "unreadable task descriptor",
                        "detail": error.to_string(),
                    })),
                )
                .await?;
            bus.hint(wake_id);
            return Ok(());
        }
    };

    let epoch = epoch_rx.borrow_and_update().clone();
    let Some(resumer) = epoch.resumer else {
        runtime
            .retry_task(
                &task,
                Utc::now() + retry_delay(task.attempt_count),
                "gateway connection has no task resumer",
            )
            .await?;
        return Ok(());
    };

    match resumer.resume(&descriptor).await {
        Ok(Some(handle)) => {
            let mut wait = Box::pin(handle.wait());
            let result = loop {
                tokio::select! {
                    result = &mut wait => break result,
                    () = tokio::time::sleep(TASK_CLAIM_RENEW_INTERVAL) => {
                        runtime
                            .renew_task_claim(task.agent_task_id, TASK_CLAIM_DURATION)
                            .await?;
                    }
                }
            };
            let outcome = result.result();
            let status = result.status();
            let error_kind = outcome.error().map(|error| error.kind());
            let transient = is_transient_task_failure(status, error_kind);
            if transient {
                runtime
                    .retry_task(
                        &task,
                        Utc::now() + retry_delay(task.attempt_count),
                        &outcome.output().render(),
                    )
                    .await?;
                return Ok(());
            }
            let payload = json_object(
                serde_json::json!({
                    "output": outcome.output().render(),
                    "delivered": "watcher",
                }),
                "task result",
            )?;
            let wake_id = if outcome.is_error_kind(ToolErrorKind::NotFound)
                || status == ToolTaskStatus::Cancelled
            {
                runtime.fail_task(&task, payload).await?
            } else {
                runtime
                    .resolve_task(&task, payload, outcome.is_error())
                    .await?
            };
            bus.hint(wake_id);
        }
        Ok(None) => {
            let wake_id = runtime
                .fail_task(
                    &task,
                    wrapped_json(serde_json::json!({
                        "error": "no task resumer accepted the canonical descriptor",
                    })),
                )
                .await?;
            bus.hint(wake_id);
        }
        Err(error) => {
            runtime
                .retry_task(
                    &task,
                    Utc::now() + retry_delay(task.attempt_count),
                    &error.to_string(),
                )
                .await?;
        }
    }
    Ok(())
}

fn retry_delay(attempt_count: i64) -> TimeDelta {
    let exponent = u32::try_from(attempt_count.max(0))
        .unwrap_or(u32::MAX)
        .min(6);
    TimeDelta::seconds(i64::from(2u32.saturating_pow(exponent)))
}

fn is_transient_task_failure(status: ToolTaskStatus, error_kind: Option<ToolErrorKind>) -> bool {
    matches!(
        error_kind,
        Some(ToolErrorKind::Network | ToolErrorKind::Timeout)
    ) || (error_kind == Some(ToolErrorKind::Cancelled) && status != ToolTaskStatus::Cancelled)
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

    #[test]
    fn backend_cancellation_is_terminal_but_transport_cancellation_retries() {
        assert!(!is_transient_task_failure(
            ToolTaskStatus::Cancelled,
            Some(ToolErrorKind::Cancelled)
        ));
        assert!(is_transient_task_failure(
            ToolTaskStatus::Failed,
            Some(ToolErrorKind::Cancelled)
        ));
        assert!(is_transient_task_failure(
            ToolTaskStatus::Failed,
            Some(ToolErrorKind::Network)
        ));
    }
}
