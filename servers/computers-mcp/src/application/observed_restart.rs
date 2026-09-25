//! One request-triggered provider read; no background restart polling or mutation.
use super::{Application, ApplicationError, Result};
use std::time::{Duration, Instant};
use veoveo_computers::{Computer, ComputerError, ReachedPhase, ReachedState, api::ComputerPhase};
use veoveo_computers_runtime::{Binding, Phase};

impl Application {
    /// Call only after current owner/grantee authorization for this Computer.
    pub(crate) async fn refresh_run(&self, before: &Computer) -> Result<bool> {
        if before.phase != ComputerPhase::Ready || before.active_operation.is_some() {
            return Ok(false);
        }
        // Offline inventory and durable admission keep their existing behavior.
        let Ok(runtime) = self.runtime.current() else {
            return Ok(false);
        };
        if runtime.provider_instance_id() != before.provider_instance_id {
            return Err(ApplicationError::Unavailable);
        }
        let binding = Binding::from_instance(
            before.computer_id,
            before.instance_id(),
            before.template_fingerprint.clone(),
        )
        .map_err(|_| ApplicationError::Configuration)?;
        let checked_at = Instant::now();
        let observed = tokio::time::timeout(Duration::from_secs(5), runtime.get(&binding))
            .await
            .map_err(|_| ApplicationError::Unavailable)?
            .map_err(|_| ApplicationError::Unavailable)?
            .ok_or(ComputerError::StateConflict)?;
        if observed.phase != Phase::Ready
            || Some(&observed.sandbox_id) != before.provider_resource_id.as_ref()
        {
            return Err(ComputerError::StateConflict.into());
        }
        if Some(&observed.main_process_instance_id) == before.process_id.as_ref() {
            return Ok(false);
        }
        self.store
            .record_observed_restart(
                before,
                ReachedState {
                    computer_id: before.computer_id,
                    provider_instance_id: runtime.provider_instance_id(),
                    replacement_instance_id: before.replacement_instance_id,
                    template_fingerprint: before.template_fingerprint.clone(),
                    resource_id: observed.sandbox_id,
                    process_id: observed.main_process_instance_id,
                    phase: ReachedPhase::Ready,
                },
                checked_at,
            )
            .await?;
        Ok(true)
    }
}
