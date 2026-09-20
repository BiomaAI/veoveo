use std::time::Duration;

use surrealdb::types::{RecordId, SurrealValue, Value};
use uuid::Uuid;

use super::*;
use crate::store::primary_transaction_error;

impl PlatformStore {
    pub async fn managed_agent_reconciliation(
        &self,
        claim: &ManagedAgentClaim,
    ) -> Result<ManagedAgentReconciliation> {
        self.managed_query(claim.clone(),
            "IF !fn::managed_agent_claim($command) { THROW 'agent_conflict'; }; LET $instance = SELECT * FROM ONLY $command.instance; LET $revision = SELECT * FROM ONLY $instance.requested_revision; LET $runtime = array::first(SELECT * FROM agent WHERE tenant = $instance.tenant AND agent_key = $instance.key LIMIT 1); RETURN { instance: $instance, revision: $revision, runtime: $runtime, episode_running: (($runtime != NONE AND $runtime.last_episode.state = 'running') ?? false), tenant_key: $instance.tenant.slug, context_key: $instance.work_context.context_key, enabled: fn::managed_agent_enabled($instance.id) };"
        ).await
    }

    pub async fn release_managed_agent_claim(&self, claim: &ManagedAgentClaim) -> Result<()> {
        let _: bool = self.managed_query(claim.clone(),
            "IF !fn::managed_agent_claim($command) { RETURN false; }; UPDATE ONLY $command.operation SET lease_owner = NONE, lease_expires_at = NONE; RETURN true;"
        ).await?;
        Ok(())
    }

    /// Internal controller inventory; no user-facing route may expose this method.
    pub async fn pending_managed_agent_operations(
        &self,
        namespace: &str,
        limit: u32,
        include_settled: bool,
    ) -> Result<Vec<ManagedAgentOperation>> {
        if !(1..=200).contains(&limit) {
            return Err(AgentManagementError::Invalid("page"));
        }
        #[derive(Clone, SurrealValue)]
        struct Inventory {
            namespace: String,
            limit: u32,
            include_settled: bool,
        }
        self.managed_query(Inventory { namespace: namespace.to_owned(), limit, include_settled },
            "RETURN SELECT * FROM managed_agent_operation WHERE instance.resources.namespace = $command.namespace AND phase NOT IN ['archived', 'failed', 'superseded'] AND ($command.include_settled OR phase NOT IN ['ready', 'paused']) AND (lease_expires_at = NONE OR lease_expires_at <= time::now()) ORDER BY created_at LIMIT $command.limit;"
        ).await
    }

    pub async fn claim_managed_agent_operation(
        &self,
        operation: RecordId,
        owner: Uuid,
    ) -> Result<Option<ManagedAgentOperation>> {
        #[derive(Clone, SurrealValue)]
        struct Claim {
            operation: RecordId,
            owner: Uuid,
        }
        self.managed_query(Claim { operation, owner },
            "LET $op = SELECT * FROM ONLY $command.operation; IF $op = NONE OR $op.phase IN ['archived', 'failed', 'superseded'] OR ($op.lease_expires_at != NONE AND $op.lease_expires_at > time::now()) OR $op.instance.operation != $op.id { RETURN NONE; }; RETURN UPDATE ONLY $op.id SET lease_owner = $command.owner, lease_fence += 1, lease_expires_at = time::now() + 30s, updated_at = time::now();"
        ).await
    }

    pub async fn renew_managed_agent_claim(&self, claim: &ManagedAgentClaim) -> Result<()> {
        let _: bool = self.managed_query(claim.clone(),
            "IF !fn::managed_agent_claim($command) { THROW 'agent_conflict'; }; UPDATE ONLY $command.operation SET lease_expires_at = time::now() + 30s; RETURN true;"
        ).await?;
        Ok(())
    }

    pub async fn claimed_managed_agent(
        &self,
        claim: &ManagedAgentClaim,
    ) -> Result<ManagedAgentInstance> {
        self.managed_query(claim.clone(),
            "IF !fn::managed_agent_claim($command) { THROW 'agent_conflict'; }; RETURN SELECT * FROM ONLY $command.instance;"
        ).await
    }

    /// Correlate a Secret's existing/generated public key. A retry must present
    /// the same key; reconciliation can never rotate an uncertain credential.
    pub async fn register_managed_agent_key(
        &self,
        claim: &ManagedAgentClaim,
        key: ManagedAgentPublicKey,
    ) -> Result<()> {
        validation::text(&key.kid, 200)?;
        for component in [&key.n, &key.e] {
            if component.is_empty()
                || component.len() > 2048
                || !component
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                return Err(AgentManagementError::Invalid("public key"));
            }
        }
        #[derive(Clone, SurrealValue)]
        struct Register {
            claim: ManagedAgentClaim,
            key: ManagedAgentPublicKey,
        }
        let _: bool = self.managed_query(Register { claim: claim.clone(), key },
            "IF !fn::managed_agent_claim($command.claim) { THROW 'agent_conflict'; }; LET $instance = SELECT * FROM ONLY $command.claim.instance; IF $instance.desired = 'archived' OR ($instance.public_key != NONE AND $instance.public_key != $command.key) { THROW 'agent_conflict'; }; UPDATE ONLY $instance.id SET public_key = $command.key; RETURN true;"
        ).await?;
        Ok(())
    }

    /// Phase and generation activation are one fenced commit. External resource
    /// ownership checks must have succeeded before the controller reports them.
    pub async fn observe_managed_agent(
        &self,
        claim: &ManagedAgentClaim,
        phase: ManagedAgentPhase,
        message: Option<String>,
    ) -> Result<ManagedAgentOperation> {
        if phase == ManagedAgentPhase::Superseded || phase == ManagedAgentPhase::Queued {
            return Err(AgentManagementError::Invalid("observation"));
        }
        if let Some(message) = &message {
            validation::text(message, 500)?;
        }
        #[derive(Clone, SurrealValue)]
        struct Observation {
            claim: ManagedAgentClaim,
            phase: ManagedAgentPhase,
            message: Option<String>,
        }
        self.managed_query(
            Observation {
                claim: claim.clone(),
                phase,
                message,
            },
            include_str!("observe.surql"),
        )
        .await
    }

    /// This helper is restricted to trusted service callers. HTTP management uses
    /// agent_query so current human/context authority is checked transactionally.
    pub(super) async fn managed_query<C: SurrealValue + Clone, R: SurrealValue>(
        &self,
        command: C,
        sql: &'static str,
    ) -> Result<R> {
        let query = format!("BEGIN TRANSACTION; {sql} COMMIT TRANSACTION;");
        for attempt in 0..8 {
            let mut response = self
                .client()
                .query(query.clone())
                .bind(("command", command.clone()))
                .await
                .map_err(|_| AgentManagementError::Unavailable)?;
            if let Some(error) = primary_transaction_error(response.take_errors()) {
                if super::super::retryable(&error) && attempt < 7 {
                    tokio::time::sleep(Duration::from_millis(1 << attempt)).await;
                    continue;
                }
                return Err(super::super::classify(&error));
            }
            let last = response
                .num_statements()
                .checked_sub(2)
                .ok_or(AgentManagementError::Unavailable)?;
            let value: Value = response
                .take(last)
                .map_err(|_| AgentManagementError::Unavailable)?;
            return R::from_value(value).map_err(|_| AgentManagementError::Unavailable);
        }
        Err(AgentManagementError::Unavailable)
    }
}
