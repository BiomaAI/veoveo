use super::*;
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;
impl<G: Preflight> LifecycleWorker<G> {
    /// Bounded concurrent work; one unavailable provider watch cannot block the
    /// whole queue. Dropping/shutting down a worker retains every durable fence.
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
                        Ok((id, result)) => {
                            active.remove(&id);
                            if result.is_err() { tracing::warn!(operation_id = %id, "Computer worker step deferred"); }
                        }
                        Err(_) => {
                            // A panic loses local identity. Abort this batch and let
                            // durable leases/fences govern the next pass.
                            jobs.abort_all();
                            while jobs.join_next().await.is_some() {}
                            active.clear();
                        }
                    }
                }
                _ = scan.tick(), if jobs.len() < 16 => {
                    let operations = match tokio::time::timeout(Duration::from_secs(5), self.store.pending_operations(cursor, 100)).await {
                        Ok(Ok(operations)) => operations,
                        _ => { tracing::warn!("Computer operation queue is unavailable"); continue; }
                    };
                    if operations.is_empty() { cursor = None; continue; }
                    for operation in operations {
                        if jobs.len() == 16 { break; }
                        cursor = Some(operation.operation_id);
                        if !active.insert(operation.operation_id) { continue; }
                        let worker = self.clone();
                        jobs.spawn(async move {
                            let id = operation.operation_id;
                            (id, worker.step(operation).await)
                        });
                    }
                }
            }
        }
        jobs.abort_all();
        while jobs.join_next().await.is_some() {}
    }
}
