//! Managed generations share the existing scheduler lease and episode ownership.
use super::*;
use crate::ManagedRuntimeBinding;
use veoveo_platform_store::agent_management::instances::ManagedKernelReady;

impl AgentRuntime {
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
