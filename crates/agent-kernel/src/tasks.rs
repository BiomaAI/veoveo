//! Task watchers: the kernel side of a detached MCP task.
//!
//! A watcher owns one ledger task from detach to settlement. It rehydrates a
//! live handle from the persisted descriptor via the current connection
//! epoch's `McpTaskResumer` — the same path whether the task detached seconds
//! ago or before a process restart — awaits the result, records it, and
//! signals the wake loop. Transport-class failures (network, timeout,
//! cancellation from a dying connection) wait for the next connection epoch
//! and re-resume; terminal failures settle the ledger row.

use std::time::Duration;

use rig_core::tool::{TaskResumer, ToolFailureKind, ToolTaskDescriptor};
use tokio::sync::{mpsc, watch};

use crate::{
    connection::ConnectionEpoch,
    ledger::{KernelLedger, TaskState, WatchableTask},
};

/// Slice-1 wake signal: a detached task settled (resolved, failed, expired).
#[derive(Debug, Clone)]
pub struct TaskSettled {
    pub task_id: String,
}

const MAX_ATTEMPTS_PER_EPOCH: u32 = 8;

pub fn arm_watcher(
    ledger: KernelLedger,
    wake_tx: mpsc::Sender<TaskSettled>,
    epoch_rx: watch::Receiver<ConnectionEpoch>,
    task: WatchableTask,
) {
    tokio::spawn(watch_task(ledger, wake_tx, epoch_rx, task));
}

async fn watch_task(
    ledger: KernelLedger,
    wake_tx: mpsc::Sender<TaskSettled>,
    mut epoch_rx: watch::Receiver<ConnectionEpoch>,
    task: WatchableTask,
) {
    let task_id = task.task_id.clone();
    let mut descriptor: ToolTaskDescriptor = match serde_json::from_str(&task.descriptor_json) {
        Ok(descriptor) => descriptor,
        Err(err) => {
            tracing::error!(task_id, %err, "task descriptor is unreadable; expiring");
            settle_expired(&ledger, &wake_tx, &task_id, "unreadable descriptor").await;
            return;
        }
    };

    let mut stripped_server_key = false;
    let mut attempts: u32 = 0;
    loop {
        let epoch = epoch_rx.borrow_and_update().clone();
        let Some(resumer) = epoch.resumer else {
            if epoch_rx.changed().await.is_err() {
                return;
            }
            continue;
        };

        match resumer.resume(&descriptor).await {
            Ok(Some(handle)) => {
                if let Err(err) = ledger.set_task_state(&task_id, TaskState::Watching) {
                    tracing::error!(task_id, %err, "marking task watching failed");
                }
                let result = handle.wait().await;
                let outcome = result.outcome();
                let transport_failure = outcome.is_error_kind(ToolFailureKind::Network)
                    || outcome.is_error_kind(ToolFailureKind::Timeout)
                    || outcome.is_error_kind(ToolFailureKind::Cancelled);
                if transport_failure {
                    attempts += 1;
                    tracing::warn!(
                        task_id,
                        attempts,
                        output = result.model_output(),
                        "task wait hit a transport failure; will re-resume"
                    );
                    if wait_for_retry(&mut epoch_rx, &mut attempts).await.is_err() {
                        return;
                    }
                    continue;
                }
                if outcome.is_error_kind(ToolFailureKind::NotFound) {
                    settle_expired(&ledger, &wake_tx, &task_id, result.model_output()).await;
                    return;
                }
                let result_json = serde_json::json!({
                    "output": result.model_output(),
                    "delivered": "watcher",
                })
                .to_string();
                if let Err(err) = ledger.resolve_task(&task_id, &result_json, outcome.is_error()) {
                    tracing::error!(task_id, %err, "recording task result failed");
                }
                tracing::info!(task_id, is_error = outcome.is_error(), "task settled");
                let _ = wake_tx.send(TaskSettled { task_id }).await;
                return;
            }
            Ok(None) => {
                // The resumer declined the descriptor. A server_key minted by
                // an older server build is the benign cause: strip it once and
                // let the gateway's task-id routing decide.
                if !stripped_server_key && descriptor.server_key.is_some() {
                    stripped_server_key = true;
                    descriptor.server_key = None;
                    continue;
                }
                settle_expired(&ledger, &wake_tx, &task_id, "no resumer accepted the task").await;
                return;
            }
            Err(err) => {
                attempts += 1;
                tracing::warn!(task_id, attempts, %err, "task resume failed");
                if attempts >= MAX_ATTEMPTS_PER_EPOCH {
                    settle_expired(&ledger, &wake_tx, &task_id, &err.to_string()).await;
                    return;
                }
                if wait_for_retry(&mut epoch_rx, &mut attempts).await.is_err() {
                    return;
                }
            }
        }
    }
}

/// Back off, but wake immediately (and reset the attempt budget) when the
/// connection rotates — a fresh sink is the most likely fix.
async fn wait_for_retry(
    epoch_rx: &mut watch::Receiver<ConnectionEpoch>,
    attempts: &mut u32,
) -> Result<(), ()> {
    if *attempts >= MAX_ATTEMPTS_PER_EPOCH {
        match epoch_rx.changed().await {
            Ok(()) => {
                *attempts = 0;
                Ok(())
            }
            Err(_) => Err(()),
        }
    } else {
        let backoff = Duration::from_secs(u64::from(2u32.saturating_pow((*attempts).min(5))));
        tokio::select! {
            changed = epoch_rx.changed() => match changed {
                Ok(()) => {
                    *attempts = 0;
                    Ok(())
                }
                Err(_) => Err(()),
            },
            () = tokio::time::sleep(backoff) => Ok(()),
        }
    }
}

async fn settle_expired(
    ledger: &KernelLedger,
    wake_tx: &mpsc::Sender<TaskSettled>,
    task_id: &str,
    reason: &str,
) {
    if let Err(err) = ledger.expire_task(task_id, reason) {
        tracing::error!(task_id, %err, "expiring task failed");
    }
    let _ = wake_tx
        .send(TaskSettled {
            task_id: task_id.to_string(),
        })
        .await;
}
