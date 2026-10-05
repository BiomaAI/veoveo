//! Managed generations share the existing scheduler lease and episode ownership.
use super::*;
use crate::ManagedRuntimeBinding;
use crate::persistence::AgentRepository;
use crate::persistence::instances::ManagedKernelReady;

impl AgentRuntime {
    pub fn lease_fence(&self) -> Result<i64> {
        self.fence()
    }

    /// Observe a live episode fence. Notifications reduce stop latency; the
    /// native feed recovers missed changes; lease expiry arms the only timer.
    pub async fn wait_for_managed_dispatch_revocation(
        &self,
        binding: &crate::persistence::instances::ManagedEpisodeBinding,
    ) -> Result<()> {
        let cursor = self.store.changefeed_cursor_now().await?;
        let mut changes = self.store.observe_changes(
            vec![
                veoveo_modules::ObservationTable::from(crate::AgentObservationTable::ManagedAgent),
                veoveo_modules::ObservationTable::from(
                    crate::AgentObservationTable::AgentDefinition,
                ),
                veoveo_modules::ObservationTable::from(PlatformTable::Principal),
                veoveo_modules::ObservationTable::from(PlatformTable::Tenant),
                veoveo_modules::ObservationTable::from(PlatformTable::WorkContext),
                veoveo_modules::ObservationTable::from(crate::AgentObservationTable::Agent),
            ],
            cursor,
        );
        changes.next().await.ok_or(AgentRuntimeError::LeaseLost)??;
        loop {
            // Read the lease before admission. A concurrent renewal emits an Agent
            // change and rearms this earlier deadline; expiry always rechecks SQL.
            let lease = self
                .agent_record()
                .await?
                .lease_expires_at
                .ok_or(AgentRuntimeError::LeaseLost)?;
            if !AgentRepository::new(self.store.clone())
                .managed_agent_kernel_dispatch(
                    binding.instance.clone(),
                    binding.generation,
                    binding.epoch,
                    self.instance_id.as_uuid(),
                    self.fence()?,
                )
                .await
                .map_err(|_| AgentRuntimeError::InvalidField {
                    field: "managed dispatch",
                    reason: "current authority is unavailable".into(),
                })?
            {
                return Ok(());
            }
            let delay = (lease - Utc::now()).to_std().unwrap_or(Duration::ZERO);
            tokio::select! {
                () = tokio::time::sleep(delay) => {},
                change = changes.next() => { change.ok_or(AgentRuntimeError::LeaseLost)??; },
            }
        }
    }

    pub async fn managed_scheduler_mode(&self) -> Result<crate::ManagedSchedulerMode> {
        let Some(binding) = &self.managed else {
            return Ok(crate::ManagedSchedulerMode::Running);
        };
        let mut response = self
            .store
            .client()
            .query(include_str!(
                "../queries/runtime/managed/managed_scheduler_mode.surql"
            ))
            .bind(("id", binding.instance.clone()))
            .bind(("generation", binding.generation))
            .await?
            .check()?;
        let mode: Option<crate::ManagedSchedulerMode> = response.take(1)?;
        mode.ok_or(AgentRuntimeError::InvalidField {
            field: "managed scheduler mode",
            reason: "missing current state".into(),
        })
    }

    pub fn with_managed_binding(mut self, binding: ManagedRuntimeBinding) -> Result<Self> {
        if binding.instance.table.as_str() != "managed_agent" || binding.generation < 1 {
            return Err(AgentRuntimeError::InvalidField {
                field: "managed binding",
                reason: "requires a managed instance and positive generation".into(),
            });
        }
        self.managed = Some(binding);
        Ok(self)
    }

    /// Readiness means this pod has installed the current gateway connection and
    /// tools. A Running Kubernetes container alone cannot satisfy this check.
    pub async fn managed_ready(&self, pod_uid: uuid::Uuid) -> Result<()> {
        let binding = self
            .managed
            .as_ref()
            .ok_or(AgentRuntimeError::InvalidField {
                field: "managed binding",
                reason: "readiness requires a managed instance".into(),
            })?;
        let fence = self.fence()?;

        self.store
            .client()
            .query(include_str!(
                "../queries/runtime/managed/managed_ready.surql"
            ))
            .bind(("instance", binding.instance.clone()))
            .bind(("generation", binding.generation))
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind((
                "ready",
                ManagedKernelReady {
                    generation: binding.generation,
                    pod_uid,
                },
            ))
            .await?
            .check()?;
        Ok(())
    }
}
