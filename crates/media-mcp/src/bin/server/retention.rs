use std::{sync::Arc, time::Duration};

use chrono::Utc;

use super::AppState;

pub(super) async fn run_retention_gc(state: &AppState) -> anyhow::Result<()> {
    let usage_cutoff = state.retention.usage_cutoff(Utc::now())?;
    let pruned_tasks = state.tasks.prune_expired().await?;
    let pruned_contexts = state.durable.prune_task_contexts().await?;
    let pruned_usage = state.durable.delete_usage_before(usage_cutoff).await?;
    tracing::info!(
        pruned_tasks = pruned_tasks.len(),
        pruned_contexts,
        pruned_usage,
        "media retention reconciliation completed"
    );
    Ok(())
}

pub(super) fn spawn_retention_gc_loop(state: Arc<AppState>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
            if let Err(error) = run_retention_gc(&state).await {
                tracing::error!("media retention reconciliation failed: {error}");
            }
        }
    });
}
