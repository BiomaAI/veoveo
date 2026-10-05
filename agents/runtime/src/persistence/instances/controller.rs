use crate::persistence::AgentRepository;
use std::time::Duration;

use surrealdb::types::{RecordId, SurrealValue, Value};
use uuid::Uuid;

use super::*;
use veoveo_platform_store::primary_transaction_error;

impl AgentRepository {
    pub async fn managed_agent_reconciliation(
        &self,
        claim: &ManagedAgentClaim,
    ) -> Result<ManagedAgentReconciliation> {
        self.managed_query(
            claim.clone(),
            include_str!("../queries/instances/controller/managed_agent_reconciliation.surql"),
        )
        .await
    }

    pub async fn release_managed_agent_claim(&self, claim: &ManagedAgentClaim) -> Result<()> {
        let _: bool = self
            .managed_query(
                claim.clone(),
                include_str!("../queries/instances/controller/release_managed_agent_claim.surql"),
            )
            .await?;
        Ok(())
    }

    /// Internal controller inventory; no user-facing route may expose this method.
    pub async fn pending_managed_agent_operations(
        &self,
        namespace: &str,
        limit: u32,
        include_settled: bool,
        after: Option<ManagedAgentOperationCursor>,
    ) -> Result<Vec<ManagedAgentOperation>> {
        if !(1..=200).contains(&limit) {
            return Err(AgentManagementError::Invalid("page"));
        }
        #[derive(Clone, SurrealValue)]
        struct Inventory {
            namespace: String,
            limit: u32,
            include_settled: bool,
            after: Option<RecordId>,
        }
        self.managed_query(
            Inventory {
                namespace: namespace.to_owned(),
                limit,
                include_settled,
                after: after.map(ManagedAgentOperationCursor::record),
            },
            include_str!("../queries/instances/controller/pending_managed_agent_operations.surql"),
        )
        .await
    }

    /// Next known claim, startup or draining-runtime deadline, using database time.
    pub async fn next_managed_agent_delay(&self, namespace: &str) -> Result<Option<Duration>> {
        #[derive(SurrealValue)]
        struct Deadlines {
            now: chrono::DateTime<chrono::Utc>,
            claim: Option<chrono::DateTime<chrono::Utc>>,
            startup: Option<chrono::DateTime<chrono::Utc>>,
            drain: Option<chrono::DateTime<chrono::Utc>>,
        }
        let times: Deadlines = self
            .managed_query(
                namespace.to_owned(),
                include_str!("../queries/instances/deadlines.surql"),
            )
            .await?;
        Ok(times
            .claim
            .into_iter()
            .chain(times.startup)
            .chain(times.drain)
            .min()
            .map(|due| (due - times.now).to_std().unwrap_or(Duration::ZERO)))
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
        self.managed_query(
            Claim { operation, owner },
            include_str!("../queries/instances/controller/claim_managed_agent_operation.surql"),
        )
        .await
    }

    pub async fn renew_managed_agent_claim(&self, claim: &ManagedAgentClaim) -> Result<()> {
        let _: bool = self
            .managed_query(
                claim.clone(),
                include_str!("../queries/instances/controller/renew_managed_agent_claim.surql"),
            )
            .await?;
        Ok(())
    }

    pub async fn claimed_managed_agent(
        &self,
        claim: &ManagedAgentClaim,
    ) -> Result<ManagedAgentInstance> {
        self.managed_query(
            claim.clone(),
            include_str!("../queries/instances/controller/claimed_managed_agent.surql"),
        )
        .await
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
        let _: bool = self
            .managed_query(
                Register {
                    claim: claim.clone(),
                    key,
                },
                include_str!("../queries/instances/controller/register_managed_agent_key.surql"),
            )
            .await?;
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
            include_str!("../queries/instances/observe.surql"),
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
        for attempt in 0..8 {
            let mut response = self
                .client()
                .query(include_str!("../queries/managed_transaction_begin.surql"))
                .query(sql)
                .query(include_str!("../queries/transaction_commit.surql"))
                .bind(("command", command.clone()))
                .await
                .map_err(|_| AgentManagementError::Unavailable)?;
            if let Some(error) = primary_transaction_error(response.take_errors()) {
                if super::super::retryable_agent_transaction(&error) && attempt < 7 {
                    tokio::time::sleep(Duration::from_millis(1 << attempt)).await;
                    continue;
                }
                return Err(super::super::classify_agent_transaction(&error));
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
