//! Prove settlement or replacement ownership after a finite recovery handoff.
use crate::{TaskError, TaskRuntime, TaskSnapshot};

impl TaskRuntime {
    /// A claim error does not prove that another worker took responsibility.
    /// Reconcile current durable state without retrying or changing the Task.
    pub async fn reconcile_recovery_claim(
        &self,
        admitted: &TaskSnapshot,
        error: anyhow::Error,
    ) -> anyhow::Result<()> {
        match error.downcast_ref::<TaskError>() {
            Some(
                TaskError::Conflict(_)
                | TaskError::LeaseHeld(_)
                | TaskError::NotFound(_)
                | TaskError::InvalidTransition { .. },
            ) => {}
            _ => return Err(error),
        }
        let Some(current) = self.get_for_recovery(admitted.task_id).await? else {
            return Ok(());
        };
        if !(admitted.task_id == current.task_id
            && admitted.server == current.server
            && admitted.task_type == current.task_type
            && admitted.recovery_class == current.recovery_class
            && admitted.owner == current.owner
            && admitted.request == current.request)
        {
            return Err(error.context("recovered Task changed after admission"));
        }
        if current.is_terminal() {
            return Ok(());
        }
        if current
            .lease_owner
            .as_deref()
            .is_some_and(|worker| worker != self.worker_id())
            && current
                .lease_expires_at
                .is_some_and(|expiry| expiry > chrono::Utc::now())
        {
            tracing::info!(task_id = %admitted.task_id, "another replica holds recovered Task lease");
            return Ok(());
        }
        Err(error.context("recovered Task has no proven live replacement worker"))
    }
}
