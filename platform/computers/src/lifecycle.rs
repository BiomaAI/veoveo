//! One dispatch receipt, a persisted observation budget, and correlated settlement.
use crate::{ComputerError, ComputersStore, Operation, OperationStage, Result, api::Action};
use chrono::{DateTime, Utc};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

pub struct DispatchTicket {
    operation: Box<Operation>,
    id: Uuid,
    deadline: Instant,
    authority_deadline: Instant,
}
impl DispatchTicket {
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    /// Fresh dispatch authority includes all policy and journal read latency.
    pub fn authority_remaining(&self) -> Duration {
        self.authority_deadline
            .saturating_duration_since(Instant::now())
    }
    pub fn authority_deadline(&self) -> Instant {
        self.authority_deadline
    }
}
pub struct ObservationTicket {
    operation: Box<Operation>,
    id: Uuid,
    deadline: Instant,
}
impl ObservationTicket {
    pub fn operation(&self) -> &Operation {
        &self.operation
    }
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
}
pub enum ObservationAdmission {
    Read(ObservationTicket),
    Wait { until: DateTime<Utc> },
    RecoveryRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReachedPhase {
    Ready,
    Stopped,
}

/// Internal provider-checked outcome. Public handlers cannot submit this as evidence.
pub struct ReachedState {
    pub provider_instance_id: Uuid,
    pub computer_id: Uuid,
    pub template_fingerprint: String,
    pub resource_id: String,
    pub process_id: String,
    pub phase: ReachedPhase,
}
impl ReachedState {
    fn check(&self, operation: &Operation) -> Result<&'static str> {
        let valid_id = |value: &str| {
            !value.is_empty()
                && value.len() <= 256
                && value.bytes().all(|byte| byte.is_ascii_graphic())
        };
        if self.provider_instance_id != operation.provider_instance_id
            || self.computer_id != operation.computer_id
            || self.template_fingerprint != operation.template_fingerprint
            || !valid_id(&self.resource_id)
            || !valid_id(&self.process_id)
        {
            return Err(ComputerError::StateConflict);
        }
        match (operation.action, self.phase) {
            (Action::Create | Action::Start, ReachedPhase::Ready) => Ok("ready"),
            (Action::Stop, ReachedPhase::Stopped) => Ok("stopped"),
            _ => Err(ComputerError::StateConflict),
        }
    }
}

impl ComputersStore {
    /// Check current action authority here; the caller prepares the retained home.
    /// A lost reply returns no ticket. Recovery can observe, but cannot redispatch.
    pub async fn begin_dispatch(&self, claimed: &ClaimedTask) -> Result<DispatchTicket> {
        let mut before = self.worker_operation(claimed).await?.operation;
        if before.stage != OperationStage::Queued {
            return Err(ComputerError::StateConflict);
        }
        let permit = self.authorize_execution(&before).await?;
        let evidence = serde_json::from_value::<veoveo_platform_store::OpenObject>(
            serde_json::to_value(&permit.evidence).map_err(|_| ComputerError::Unavailable)?,
        )
        .map_err(|_| ComputerError::Unavailable)?;
        let expires_at = permit.evidence.valid_until;
        before.dispatch_authority = Some(permit.evidence);
        let id = Uuid::now_v7();
        self.worker_commit(
            claimed,
            &before,
            ProviderCommit::Dispatch,
            include_str!("../queries/dispatch.surql"),
            vec![
                ("dispatch_id", id.into_value()),
                ("dispatch_authority", evidence.into_value()),
                ("authority_expires_at", expires_at.into_value()),
                ("authority_revision", permit.revision_record.into_value()),
                (
                    "authority_enterprise",
                    veoveo_platform_store::deterministic_enterprise_id()
                        .record_id()
                        .into_value(),
                ),
                ("authority_tenant", permit.tenant.into_value()),
                ("authority_source", permit.source.into_value()),
                ("authority_actor", permit.actor.into_value()),
            ],
            "computer.operation_dispatched",
        )
        .await?;
        let after = self.worker_operation(claimed).await?;
        let remaining = after.remaining();
        let operation = after.operation;
        if operation.dispatch_id != Some(id) || operation.stage != OperationStage::Dispatched {
            return Err(ComputerError::StateConflict);
        }
        if remaining.is_zero() || permit.deadline <= Instant::now() {
            return Err(ComputerError::StateConflict);
        }
        Ok(DispatchTicket {
            operation: Box::new(operation),
            id,
            deadline: Instant::now() + remaining,
            authority_deadline: permit.deadline,
        })
    }

    pub async fn admit_observation(&self, claimed: &ClaimedTask) -> Result<ObservationAdmission> {
        let before = self.worker_operation(claimed).await?.operation;
        let id = Uuid::now_v7();
        // Persisted count drives exponential delay. Deterministic jitter spreads
        // different operations without a per-process retry schedule or new budget.
        let jitter = u64::from(before.operation_id.as_bytes()[15]);
        let milliseconds = (500_u64 << before.observation_reads.min(5)).min(10_000) + jitter;
        self.worker_commit(
            claimed,
            &before,
            ProviderCommit::Observe,
            include_str!("../queries/observe.surql"),
            vec![
                ("observation_id", id.into_value()),
                (
                    "backoff",
                    surrealdb::types::Duration::from_std(Duration::from_millis(milliseconds))
                        .into_value(),
                ),
            ],
            "computer.recovery_required",
        )
        .await?;
        let after = self.worker_operation(claimed).await?;
        if after.operation.stage == OperationStage::RecoveryRequired {
            return Ok(ObservationAdmission::RecoveryRequired);
        }
        if after.operation.last_observation_id == Some(id) {
            let remaining = after.remaining().min(Duration::from_secs(10));
            if remaining.is_zero() {
                return Err(ComputerError::StateConflict);
            }
            return Ok(ObservationAdmission::Read(ObservationTicket {
                operation: Box::new(after.operation),
                id,
                deadline: Instant::now() + remaining,
            }));
        }
        Ok(ObservationAdmission::Wait {
            until: after
                .operation
                .next_observation_at
                .ok_or(ComputerError::StateConflict)?,
        })
    }

    pub async fn complete_dispatch(
        &self,
        claimed: &ClaimedTask,
        ticket: DispatchTicket,
        reached: ReachedState,
    ) -> Result<Operation> {
        self.settle(claimed, &ticket.operation, "dispatch", ticket.id, reached)
            .await
    }
    pub async fn complete_observation(
        &self,
        claimed: &ClaimedTask,
        ticket: ObservationTicket,
        reached: ReachedState,
    ) -> Result<Operation> {
        self.settle(
            claimed,
            &ticket.operation,
            "observation",
            ticket.id,
            reached,
        )
        .await
    }
    async fn settle(
        &self,
        claimed: &ClaimedTask,
        ticket_operation: &Operation,
        kind: &'static str,
        id: Uuid,
        reached: ReachedState,
    ) -> Result<Operation> {
        let current = self.worker_operation(claimed).await?.operation;
        if current.operation_id != ticket_operation.operation_id {
            return Err(ComputerError::StateConflict);
        }
        let phase = reached.check(&current)?;
        self.worker_commit(
            claimed,
            &current,
            ProviderCommit::Observe,
            include_str!("../queries/settle.surql"),
            vec![
                ("evidence_kind", kind.into_value()),
                ("evidence_id", id.into_value()),
                ("resource", reached.resource_id.into_value()),
                ("process", reached.process_id.into_value()),
                ("phase", phase.into_value()),
            ],
            "computer.operation_succeeded",
        )
        .await?;
        Ok(self.worker_operation(claimed).await?.operation)
    }
}
