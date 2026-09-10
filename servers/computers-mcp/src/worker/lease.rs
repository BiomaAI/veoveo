use super::*;
use std::future::Future;
impl<G: Preflight> LifecycleWorker<G> {
    pub(super) async fn with_lease<F: Future>(
        &self,
        claimed: &mut ClaimedTask,
        future: F,
    ) -> Result<F::Output> {
        tokio::pin!(future);
        let mut renewal = tokio::time::interval(Duration::from_secs(10));
        renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        renewal.tick().await;
        loop {
            tokio::select! {
                biased;
                _ = renewal.tick() => {
                    let snapshot = tokio::time::timeout(Duration::from_secs(5), self.tasks.renew_lease(&claimed.snapshot.task_id.to_string(), LEASE_DURATION))
                        .await.map_err(|_| WorkerError::LeaseLost)??;
                    claimed.lease_expires_at = snapshot.lease_expires_at.ok_or(WorkerError::LeaseLost)?;
                    claimed.snapshot = snapshot;
                }
                output = &mut future => return Ok(output),
            }
        }
    }
}
