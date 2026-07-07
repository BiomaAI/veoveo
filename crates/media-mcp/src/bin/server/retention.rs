use std::{sync::Arc, time::Duration};

use chrono::Utc;

use super::AppState;

pub(super) async fn run_retention_gc(state: &AppState) -> anyhow::Result<()> {
    let now = Utc::now();
    let task_cutoff = state.retention.task_cutoff(now)?;
    let usage_cutoff = state.retention.usage_cutoff(now)?;

    let pruned_tasks = state.tasks.prune_terminal_before(task_cutoff).await?;
    if !pruned_tasks.is_empty() {
        let mut owners = state.task_owners.write().await;
        let mut predictions = state.predictions.write().await;
        for task in &pruned_tasks {
            owners.remove(&task.task_id);
            if let Some(provider_job_id) = &task.provider_job_id {
                predictions.remove(provider_job_id);
            }
        }
    }
    let task_summary = state.durable.delete_terminal_tasks_before(task_cutoff)?;
    let usage_deleted = state.durable.delete_usage_records_before(usage_cutoff)?;
    // Artifact retention is now owned by the shared plane (grant-scoped, with
    // crypto-shred on delete); media no longer prunes artifact bytes locally.

    tracing::info!(
        pruned_memory_tasks = pruned_tasks.len(),
        deleted_task_rows = task_summary.tasks_deleted,
        deleted_task_owner_rows = task_summary.task_owners_deleted,
        deleted_prediction_rows = task_summary.predictions_deleted,
        deleted_usage_rows = usage_deleted,
        "media retention gc completed"
    );
    Ok(())
}

pub(super) fn spawn_retention_gc_loop(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            if let Err(err) = run_retention_gc(&state).await {
                tracing::error!("media retention gc failed: {err}");
            }
        }
    });
}
