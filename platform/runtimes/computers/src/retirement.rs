//! One guarded retirement request. Physical exclusion belongs to the allocator.
use crate::{
    Binding, Observation, OpenShellRuntime, Phase, Result, RuntimeFailure, client::request,
    protocol::v1 as api,
};
use std::time::Duration;

/// Provider acknowledgement only. Neither variant proves physical writer removal
/// or grants permission to adopt a home, release capacity, or replay a mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetirementAcknowledgement {
    DeletionAccepted,
    ResourceAlreadyAbsent,
}
impl OpenShellRuntime {
    /// Caller must hold a durable maintenance fence and the original dispatch
    /// ticket. The profile excludes concurrent out-of-band provider administration
    /// and never reuses a retired instance name. Lost replies permit bounded
    /// observation and physical fencing, not another dispatch.
    pub async fn retire(
        &self,
        binding: &Binding,
        before: &Observation,
    ) -> Result<RetirementAcknowledgement> {
        if before.phase != Phase::Stopped
            || !crate::models::identifier(&before.sandbox_id)
            || !crate::models::identifier(&before.main_process_instance_id)
        {
            return Err(RuntimeFailure::BindingMismatch);
        }
        tokio::time::timeout(Duration::from_secs(30), async {
            let current = self.get(binding).await?.ok_or(RuntimeFailure::NotFound)?;
            if current != *before {
                return Err(RuntimeFailure::BindingMismatch);
            }
            let reply = self
                .client
                .clone()
                .delete_sandbox(request(
                    api::DeleteSandboxRequest {
                        name: binding.name(),
                        allow_missing: true,
                        workspace_scope: crate::client::workspace_scope(&self.workspace),
                        ..Default::default()
                    },
                    30,
                ))
                .await
                .map_err(|_| RuntimeFailure::LifecycleUnknown)?
                .into_inner();
            if reply.sandbox_id != current.sandbox_id
                && reply.outcome != api::DeletionOutcome::AlreadyAbsent as i32
            {
                return Err(RuntimeFailure::BindingMismatch);
            }
            match api::DeletionOutcome::try_from(reply.outcome) {
                Ok(api::DeletionOutcome::Completed | api::DeletionOutcome::Accepted) => {
                    Ok(RetirementAcknowledgement::DeletionAccepted)
                }
                Ok(api::DeletionOutcome::AlreadyAbsent) => {
                    Ok(RetirementAcknowledgement::ResourceAlreadyAbsent)
                }
                _ => Err(RuntimeFailure::LifecycleUnknown),
            }
        })
        .await
        .map_err(|_| RuntimeFailure::LifecycleUnknown)?
    }
}
