//! Task settlement for the existing Resume execution profile.
use crate::{RecoveryClass, TaskError, TaskRuntime, TaskSnapshot, TaskStatus, TaskTransition};
use tokio_util::sync::CancellationToken;
use veoveo_types::TaskId;

/// The Resume owner's treatment of a genuine failure concurrent with cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum ResumeCancellationPolicy {
    CancellationWins,
    PreserveFailure,
}

impl TaskRuntime {
    /// Settle Resume work under this worker's live execution lease.
    /// A stopped success returns unchanged nonterminal state unless cancellation is durable.
    /// Once a transition is entered, local stop cannot undo its database outcome.
    pub async fn transition_resumable(
        &self,
        id: TaskId,
        transition: TaskTransition,
        policy: ResumeCancellationPolicy,
        stop: Option<&CancellationToken>,
    ) -> Result<TaskSnapshot, TaskError> {
        let current = self.resume_snapshot(id).await?;
        self.resume_settle(&current, &current, transition, policy, stop)
            .await
    }

    /// As `transition_resumable`, with a caller-selected CAS snapshot. Immutable
    /// identity is admitted against current durable state before any dispatch.
    pub async fn transition_resumable_if_current(
        &self,
        current: &TaskSnapshot,
        transition: TaskTransition,
        policy: ResumeCancellationPolicy,
        stop: Option<&CancellationToken>,
    ) -> Result<TaskSnapshot, TaskError> {
        let observed = self.resume_snapshot(current.task_id).await?;
        self.resume_settle(current, &observed, transition, policy, stop)
            .await
    }

    async fn resume_snapshot(&self, id: TaskId) -> Result<TaskSnapshot, TaskError> {
        self.get(id)
            .await?
            .ok_or_else(|| TaskError::NotFound(id.to_string()))
    }

    async fn resume_settle(
        &self,
        selected: &TaskSnapshot,
        admitted: &TaskSnapshot,
        transition: TaskTransition,
        policy: ResumeCancellationPolicy,
        stop: Option<&CancellationToken>,
    ) -> Result<TaskSnapshot, TaskError> {
        if matches!(transition, TaskTransition::CancelRequested) {
            return Err(TaskError::InvalidRecord(
                "Resume execution cannot request cancellation".into(),
            ));
        }
        self.resume_identity(selected, admitted)?;
        if admitted.is_terminal() {
            return Ok(admitted.clone());
        }
        self.resume_execution_owner(admitted)?;
        match self
            .resume_dispatch(selected, admitted, transition.clone(), policy, stop)
            .await
        {
            Ok(settled) => Ok(settled),
            Err(error @ (TaskError::Conflict(_) | TaskError::InvalidTransition { .. })) => {
                let observed = self.resume_snapshot(selected.task_id).await?;
                self.resume_identity(selected, &observed)?;
                if observed.is_terminal() {
                    return Ok(observed);
                }
                self.resume_execution_owner(&observed)?;
                if observed.status != TaskStatus::CancelRequested {
                    // Shutdown-only suppression is an unchanged snapshot, never
                    // an applied success; unrelated progress conflicts propagate.
                    return Err(error);
                }
                self.resume_dispatch(&observed, &observed, transition, policy, stop)
                    .await
            }
            Err(error) => Err(error),
        }
    }

    async fn resume_dispatch(
        &self,
        current: &TaskSnapshot,
        admitted: &TaskSnapshot,
        transition: TaskTransition,
        policy: ResumeCancellationPolicy,
        stop: Option<&CancellationToken>,
    ) -> Result<TaskSnapshot, TaskError> {
        self.resume_execution_owner(current)?;
        let transition = if current.status == TaskStatus::CancelRequested
            && !(policy == ResumeCancellationPolicy::PreserveFailure
                && matches!(transition, TaskTransition::Failed(_)))
        {
            TaskTransition::Cancelled
        } else {
            transition
        };
        if stopped_success(&transition, stop) {
            // Use the admitted read even if stop arrived after that await and
            // the caller's selected version predates durable cancellation.
            self.resume_execution_owner(admitted)?;
            if admitted.status == TaskStatus::CancelRequested {
                return self
                    .transition_if_current(admitted, TaskTransition::Cancelled)
                    .await;
            }
            return Ok(admitted.clone());
        }
        self.transition_if_current(current, transition).await
    }

    fn resume_identity(
        &self,
        selected: &TaskSnapshot,
        current: &TaskSnapshot,
    ) -> Result<(), TaskError> {
        if current.server != self.server() {
            return Err(TaskError::WrongServer(current.task_id.to_string()));
        }
        if selected.recovery_class != RecoveryClass::Resume
            || current.recovery_class != RecoveryClass::Resume
        {
            return Err(TaskError::InvalidRecord(
                "Resume execution requires the Resume recovery class".into(),
            ));
        }
        if selected.task_id != current.task_id
            || selected.owner != current.owner
            || selected.request != current.request
            || selected.server != current.server
            || selected.task_type != current.task_type
            || selected.created_at != current.created_at
            || selected.ttl_ms != current.ttl_ms
            || selected.poll_interval_ms != current.poll_interval_ms
            || selected.recovery_class != current.recovery_class
        {
            return Err(TaskError::Conflict(selected.task_id.to_string()));
        }
        Ok(())
    }

    fn resume_execution_owner(&self, current: &TaskSnapshot) -> Result<(), TaskError> {
        if current.lease_owner.as_deref() != Some(self.worker_id())
            || current
                .lease_expires_at
                .is_none_or(|expiry| expiry <= chrono::Utc::now())
        {
            return Err(TaskError::LeaseHeld(current.task_id.to_string()));
        }
        Ok(())
    }
}

fn stopped_success(transition: &TaskTransition, stop: Option<&CancellationToken>) -> bool {
    matches!(transition, TaskTransition::Succeeded { .. })
        && stop.is_some_and(CancellationToken::is_cancelled)
}

#[cfg(test)]
#[path = "resumable/tests.rs"]
mod tests;
