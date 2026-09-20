//! Managed generations share the existing scheduler lease and episode ownership.
use super::*;
use crate::ManagedRuntimeBinding;
use veoveo_platform_store::agent_management::instances::ManagedKernelReady;

impl AgentRuntime {
    pub fn lease_fence(&self) -> Result<i64> {
        self.fence()
    }

    /// Observe a live episode fence. Notifications reduce stop latency; the
    /// bounded reread recovers a missed edge without any model/provider query.
    pub async fn wait_for_managed_dispatch_revocation(
        &self,
        binding: &veoveo_platform_store::agent_management::instances::ManagedEpisodeBinding,
    ) -> Result<()> {
        let mut live = self
            .store
            .live::<OutboxEventRecord>(PlatformTable::OutboxEvent)
            .await?;
        let mut recovery = tokio::time::interval(Duration::from_secs(5));
        recovery.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            if !self
                .store
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
            loop {
                tokio::select! {
                    _ = recovery.tick() => break,
                    event = live.next() => match event {
                        Some(Ok(event)) if matches!(event.data.aggregate_type.as_str(), "managed_agent" | "agent_definition" | "principal" | "work_context" | "agent") => break,
                        Some(Ok(_)) => {},
                        Some(Err(error)) => return Err(AgentRuntimeError::Database(error)),
                        None => return Err(AgentRuntimeError::LeaseLost),
                    }
                }
            }
        }
    }

    pub async fn managed_scheduler_mode(&self) -> Result<crate::ManagedSchedulerMode> {
        let Some(binding) = &self.managed else {
            return Ok(crate::ManagedSchedulerMode::Running);
        };
        let mut response = self.store.client().query(
            "LET $instance = SELECT * FROM ONLY $id; RETURN IF !fn::managed_agent_enabled($id) OR $instance.active_generation != $generation { 'retire' } ELSE IF $instance.desired = 'paused' { 'paused' } ELSE IF $instance.generation != $generation { 'retire' } ELSE { 'running' };"
        ).bind(("id", binding.instance.clone())).bind(("generation", binding.generation)).await?.check()?;
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
        let event = outbox(
            &self.identity,
            "agent",
            self.agent_id.to_string(),
            "agent.managed_ready",
            object([
                ("generation".into(), serde_json::json!(binding.generation)),
                ("pod_uid".into(), serde_json::json!(pod_uid)),
            ]),
        );
        self.store.client().query(
            "BEGIN TRANSACTION; LET $managed = SELECT * FROM ONLY $instance; IF !fn::managed_agent_enabled($instance) OR $managed.tenant != $agent.tenant OR $managed.key != $agent.agent_key OR $managed.active_generation != $generation { THROW 'managed generation is unavailable'; }; LET $ready = UPDATE ONLY $agent SET managed_ready = $ready, revision += 1, updated_at = time::now() WHERE lease_owner = $owner AND fence = $fence AND lease_expires_at > time::now() RETURN AFTER; IF $ready = NONE { THROW 'agent lease lost'; }; CREATE outbox_event CONTENT $event; COMMIT TRANSACTION;"
        ).bind(("instance", binding.instance.clone())).bind(("generation", binding.generation))
            .bind(("agent", self.agent_id.record_id())).bind(("owner", self.instance_id.to_string())).bind(("fence", fence))
            .bind(("ready", ManagedKernelReady { generation: binding.generation, pod_uid })).bind(("event", event))
            .await?.check()?;
        Ok(())
    }
}
