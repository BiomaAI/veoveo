use super::*;
use std::collections::HashSet;
use tokio_util::sync::CancellationToken;

impl MaintenanceWorker {
    /// Bounded worker discovery over the domain journal, never provider polling.
    /// Replica conflicts defer the job; only a committed ticket permits effects.
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
                        Ok((id,result)) => {
                            active.remove(&id);
                            if result.is_err() {tracing::warn!(maintenance_id=%id,"Computer maintenance step deferred");}
                        }
                        Err(_) => {jobs.abort_all(); while jobs.join_next().await.is_some() {} active.clear();}
                    }
                }
                _ = scan.tick(), if jobs.len() < 4 => {
                    let operations = match tokio::time::timeout(Duration::from_secs(5),self.store.pending_maintenance(cursor,(4-jobs.len()) as u32)).await {
                        Ok(Ok(operations)) => operations,
                        _ => {tracing::warn!("Computer maintenance queue is unavailable"); continue;}
                    };
                    if operations.is_empty() {cursor=None;continue;}
                    for operation in operations {
                        cursor=Some(operation.operation_id);
                        if !active.insert(operation.operation_id) {continue;}
                        let worker=self.clone();
                        jobs.spawn(async move {let id=operation.operation_id; (id,worker.step(operation).boxed().await)});
                    }
                }
            }
        }
        jobs.abort_all();
        while jobs.join_next().await.is_some() {}
    }
}
