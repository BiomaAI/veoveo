use super::*;
use futures::FutureExt;
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

impl CommandWorker {
    /// Four commands bound captured payload bytes to 256 MiB per replica;
    /// allocator and transport overhead are additional. Every native
    /// dispatch still requires the Computer's independent durable execution slot.
    pub async fn run(self: Arc<Self>, shutdown: CancellationToken) {
        let mut active = HashSet::new();
        let mut jobs: tokio::task::JoinSet<(uuid::Uuid, Result<WorkerStep>)> =
            tokio::task::JoinSet::new();
        let mut cursor = None;
        let mut scan = tokio::time::interval(Duration::from_secs(1));
        scan.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                Some(result) = jobs.join_next(), if !jobs.is_empty() => {
                    match result {
                        Ok((id, result)) => { active.remove(&id); if result.is_err() { tracing::warn!(execution_id = %id, "Computer command step deferred"); } },
                        Err(_) => { jobs.abort_all(); while jobs.join_next().await.is_some() {} active.clear(); },
                    }
                }
                _ = scan.tick(), if jobs.len() < 4 => {
                    let operations = match tokio::time::timeout(Duration::from_secs(5), self.store.pending_commands(cursor, (4 - jobs.len()) as u32)).await {
                        Ok(Ok(operations)) => operations,
                        _ => { tracing::warn!("Computer command queue is unavailable"); continue; },
                    };
                    if operations.is_empty() { cursor = None; continue; }
                    for operation in operations {
                        if jobs.len() == 4 { break; }
                        cursor = Some(operation.execution_id());
                        if !active.insert(operation.execution_id()) { continue; }
                        let worker = self.clone();
                        jobs.spawn(async move { let id = operation.execution_id(); (id, worker.step(operation).boxed().await) });
                    }
                }
            }
        }
        jobs.abort_all();
        while jobs.join_next().await.is_some() {}
    }
}
