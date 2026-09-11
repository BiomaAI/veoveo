use super::*;
use veoveo_task_runtime::TaskStatus;

impl CommandWorker {
    async fn current_claim(&self, claim: &ClaimedTask) -> Result<ClaimedTask> {
        let snapshot = tokio::time::timeout(
            Duration::from_secs(4),
            self.tasks.get(&claim.snapshot.task_id.to_string()),
        )
        .await
        .map_err(|_| CommandWorkerError::LeaseLost)??
        .ok_or(CommandWorkerError::LeaseLost)?;
        let expires = snapshot
            .lease_expires_at
            .ok_or(CommandWorkerError::LeaseLost)?;
        if snapshot.lease_owner.as_deref() != Some(&claim.lease_owner)
            || expires <= chrono::Utc::now()
            || snapshot.owner != claim.snapshot.owner
            || snapshot.request != claim.snapshot.request
            || snapshot.task_type != "computer.execution"
            || snapshot.is_terminal()
        {
            return Err(CommandWorkerError::LeaseLost);
        }
        Ok(ClaimedTask {
            snapshot,
            lease_expires_at: expires,
            lease_owner: claim.lease_owner.clone(),
        })
    }
    pub(super) async fn recover_lease(&self, claim: &mut ClaimedTask) -> Result<()> {
        *claim = self.current_claim(claim).await?;
        Ok(())
    }
    pub(super) async fn refresh(
        &self,
        original: &ClaimedTask,
        operation: &CommandOperation,
    ) -> Result<CommandContinuation> {
        let mut claim = self.current_claim(original).await?;
        if claim.snapshot.status == TaskStatus::CancelRequested {
            return Ok(CommandContinuation::Interrupted(
                CommandInterruption::Cancelled,
            ));
        }
        if claim.lease_expires_at - chrono::Utc::now() < chrono::TimeDelta::seconds(45) {
            let snapshot = self
                .tasks
                .renew_lease(&claim.snapshot.task_id.to_string(), LEASE_DURATION)
                .await?;
            claim.lease_expires_at = snapshot
                .lease_expires_at
                .ok_or(CommandWorkerError::LeaseLost)?;
            claim.snapshot = snapshot;
        }
        Ok(self.store.command_continuation(&claim, operation).await?)
    }
    /// Publication and containment retain their existing bounded authority. Lease
    /// renewal cannot conceal the deadline while network I/O is stalled.
    pub(super) async fn with_lease<F: Future>(
        &self,
        claim: &mut ClaimedTask,
        deadline: Instant,
        future: F,
    ) -> Result<F::Output> {
        tokio::pin!(future);
        let expires = tokio::time::sleep_until(deadline.into());
        tokio::pin!(expires);
        let mut renew = tokio::time::interval(Duration::from_secs(10));
        renew.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        renew.tick().await;
        loop {
            tokio::select! {
                biased;
                _ = &mut expires => return Err(CommandWorkerError::LeaseLost),
                output = &mut future => { self.recover_lease(claim).await?; return Ok(output); },
                snapshot = async {
                    renew.tick().await;
                    tokio::time::timeout(Duration::from_secs(4), self.tasks.renew_lease(&claim.snapshot.task_id.to_string(), LEASE_DURATION)).await
                } => {
                    let snapshot = snapshot.map_err(|_| CommandWorkerError::LeaseLost)??;
                    claim.lease_expires_at = snapshot.lease_expires_at.ok_or(CommandWorkerError::LeaseLost)?;
                    claim.snapshot = snapshot;
                }
            }
        }
    }
}
