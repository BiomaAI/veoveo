//! Durable dispatch and bounded recovery for each immutable maintenance step.
use super::{
    MaintenanceEvidence, MaintenanceOperation, MaintenanceRecovery, MaintenanceStage,
    MaintenanceStep, MaintenanceStepRecord, journal::JournalChange,
};
use crate::{ComputerError, ComputersStore, Result};
use chrono::{DateTime, TimeDelta, Utc};
use std::time::{Duration, Instant};
use uuid::Uuid;
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

pub struct MaintenanceTicket {
    pub(super) operation: Box<MaintenanceOperation>,
    dispatch_id: Uuid,
    observation_id: Option<Uuid>,
    deadline: Instant,
    authority_deadline: Option<Instant>,
}
impl MaintenanceTicket {
    pub fn operation(&self) -> &MaintenanceOperation {
        &self.operation
    }
    pub fn step(&self) -> MaintenanceStep {
        self.operation.steps().last().expect("ticket step").step
    }
    pub fn is_dispatch(&self) -> bool {
        self.observation_id.is_none()
    }
    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }
    pub fn authority_deadline(&self) -> Option<Instant> {
        self.authority_deadline
    }
}
pub enum MaintenanceObservationAdmission {
    Read(MaintenanceTicket),
    Wait { until: DateTime<Utc> },
    RecoveryRequired,
}

impl ComputersStore {
    /// An unknown commit reply yields no ticket; the next owner observes only.
    pub async fn begin_maintenance_step(&self, claim: &ClaimedTask) -> Result<MaintenanceTicket> {
        let clock = self.worker_maintenance(claim).await?;
        let before = clock.operation;
        let step = before.next_step().ok_or(ComputerError::InvalidState)?;
        let permit = self.authorize_maintenance(&before).await?;
        let mut after = before.clone();
        let dispatch_id = Uuid::now_v7();
        after.progress.steps.push(MaintenanceStepRecord {
            step,
            dispatch_id,
            authority: permit.evidence.clone(),
            dispatched_at: clock.database_time,
            observation_deadline: clock.database_time + TimeDelta::seconds(180),
            observation_reads: 0,
            last_observation_id: None,
            next_observation_at: None,
            evidence: None,
            settled_at: None,
        });
        after.stage = step.stage();
        self.commit_maintenance(
            claim,
            &before,
            &after,
            JournalChange {
                kind: ProviderCommit::Dispatch,
                permit: Some(&permit),
                event: "computer.maintenance_step_dispatched",
                checkpoint: None,
            },
        )
        .await?;
        let clock = self.worker_maintenance(claim).await?;
        let current = clock
            .operation
            .steps()
            .last()
            .ok_or(ComputerError::StateConflict)?;
        let remaining = clock.remaining(current.observation_deadline);
        if current.dispatch_id != dispatch_id
            || current.evidence.is_some()
            || remaining.is_zero()
            || permit.deadline <= Instant::now()
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(MaintenanceTicket {
            operation: Box::new(clock.operation),
            dispatch_id,
            observation_id: None,
            deadline: Instant::now() + remaining,
            authority_deadline: Some(permit.deadline),
        })
    }

    pub async fn observe_maintenance_step(
        &self,
        claim: &ClaimedTask,
    ) -> Result<MaintenanceObservationAdmission> {
        self.maintenance_observation(claim, false).await
    }

    /// Final verification shares the last step's remaining read/deadline budget.
    /// Adoption cannot acquire a fresh unbounded provider observation loop.
    pub async fn observe_maintenance_adoption(
        &self,
        claim: &ClaimedTask,
    ) -> Result<MaintenanceObservationAdmission> {
        self.maintenance_observation(claim, true).await
    }

    async fn maintenance_observation(
        &self,
        claim: &ClaimedTask,
        adoption: bool,
    ) -> Result<MaintenanceObservationAdmission> {
        let clock = self.worker_maintenance(claim).await?;
        if clock.operation.stage == MaintenanceStage::RecoveryRequired {
            return Ok(MaintenanceObservationAdmission::RecoveryRequired);
        }
        let current = clock
            .operation
            .steps()
            .last()
            .ok_or(ComputerError::InvalidState)?;
        if adoption != (clock.operation.stage == MaintenanceStage::Adopting)
            || current.evidence.is_some() != adoption
        {
            return Err(ComputerError::InvalidState);
        }
        if current.observation_reads >= 8 || clock.remaining(current.observation_deadline).is_zero()
        {
            self.pause_maintenance(claim, MaintenanceRecovery::BudgetExhausted)
                .await?;
            return Ok(MaintenanceObservationAdmission::RecoveryRequired);
        }
        if let Some(until) = current.next_observation_at
            && until > clock.database_time
        {
            return Ok(MaintenanceObservationAdmission::Wait { until });
        }
        let mut after = clock.operation.clone();
        let step = after
            .progress
            .steps
            .last_mut()
            .ok_or(ComputerError::InvalidState)?;
        let observation_id = Uuid::now_v7();
        let backoff = (500_i64 << step.observation_reads.min(5)).min(10_000)
            + i64::from(after.operation_id.as_bytes()[15]);
        step.observation_reads += 1;
        step.last_observation_id = Some(observation_id);
        step.next_observation_at = Some(clock.database_time + TimeDelta::milliseconds(backoff));
        self.commit_maintenance(
            claim,
            &clock.operation,
            &after,
            JournalChange {
                kind: ProviderCommit::Observe,
                permit: None,
                event: "computer.maintenance_observation_admitted",
                checkpoint: None,
            },
        )
        .await?;
        let clock = self.worker_maintenance(claim).await?;
        let step = clock
            .operation
            .steps()
            .last()
            .ok_or(ComputerError::StateConflict)?;
        let remaining = clock
            .remaining(step.observation_deadline)
            .min(Duration::from_secs(10));
        if step.last_observation_id != Some(observation_id)
            || step.evidence.is_some() != adoption
            || remaining.is_zero()
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(MaintenanceObservationAdmission::Read(MaintenanceTicket {
            dispatch_id: step.dispatch_id,
            operation: Box::new(clock.operation),
            observation_id: Some(observation_id),
            deadline: Instant::now() + remaining,
            authority_deadline: None,
        }))
    }

    pub async fn complete_maintenance_step(
        &self,
        claim: &ClaimedTask,
        ticket: MaintenanceTicket,
        evidence: MaintenanceEvidence,
    ) -> Result<MaintenanceOperation> {
        if evidence == MaintenanceEvidence::Captured {
            return Err(ComputerError::InvalidInput);
        }
        self.settle_maintenance_step(claim, ticket, evidence, None)
            .await
    }
    pub(super) async fn settle_maintenance_step(
        &self,
        claim: &ClaimedTask,
        ticket: MaintenanceTicket,
        evidence: MaintenanceEvidence,
        checkpoint: Option<&crate::secrets::SealedMaintenanceCheckpoint>,
    ) -> Result<MaintenanceOperation> {
        if ticket.remaining().is_zero() {
            return Err(ComputerError::StateConflict);
        }
        let clock = self.worker_maintenance(claim).await?;
        let before = clock.operation;
        let current = before.steps().last().ok_or(ComputerError::InvalidState)?;
        if before.operation_id != ticket.operation.operation_id
            || current.step != ticket.step()
            || current.dispatch_id != ticket.dispatch_id
            || current.evidence.is_some()
            || ticket
                .observation_id
                .is_some_and(|id| current.last_observation_id != Some(id))
            || before.stage == MaintenanceStage::RecoveryRequired
        {
            return Err(ComputerError::StateConflict);
        }
        evidence.check(&before, current.step)?;
        let mut after = before.clone();
        let last = after
            .progress
            .steps
            .last_mut()
            .ok_or(ComputerError::InvalidState)?;
        last.evidence = Some(evidence);
        last.settled_at = Some(clock.database_time);
        // Conclusive settlement ends recovery backoff. Final verification may
        // spend the remaining budget immediately instead of waiting up to 10s.
        if last.observation_reads > 0 {
            last.next_observation_at = Some(clock.database_time);
        }
        if after.next_step().is_none() {
            after.stage = MaintenanceStage::Adopting;
        }
        self.commit_maintenance(
            claim,
            &before,
            &after,
            JournalChange {
                kind: ProviderCommit::Observe,
                permit: None,
                event: "computer.maintenance_step_reached",
                checkpoint,
            },
        )
        .await?;
        self.maintenance_for_claim(claim).await
    }

    /// Pausing preserves both instance identities, all step receipts and the fence.
    /// It does not extend observation budgets or permit another dispatch.
    pub async fn pause_maintenance(
        &self,
        claim: &ClaimedTask,
        reason: MaintenanceRecovery,
    ) -> Result<MaintenanceOperation> {
        let before = self.maintenance_for_claim(claim).await?;
        if matches!(
            before.stage,
            MaintenanceStage::Succeeded | MaintenanceStage::Cancelled
        ) {
            return Err(ComputerError::InvalidState);
        }
        if before.stage == MaintenanceStage::RecoveryRequired {
            return Ok(before);
        }
        let mut after = before.clone();
        after.stage = MaintenanceStage::RecoveryRequired;
        after.progress.recovery = Some(reason);
        self.commit_maintenance(
            claim,
            &before,
            &after,
            JournalChange {
                kind: ProviderCommit::Observe,
                permit: None,
                event: "computer.maintenance_recovery_required",
                checkpoint: None,
            },
        )
        .await?;
        self.maintenance_for_claim(claim).await
    }
    pub async fn cancel_undispatched_maintenance(
        &self,
        claim: &ClaimedTask,
    ) -> Result<MaintenanceOperation> {
        let before = self.maintenance_for_claim(claim).await?;
        if before.stage != MaintenanceStage::Queued || !before.steps().is_empty() {
            return Err(ComputerError::InvalidState);
        }
        let mut after = before.clone();
        after.stage = MaintenanceStage::Cancelled;
        self.commit_maintenance(
            claim,
            &before,
            &after,
            JournalChange {
                kind: ProviderCommit::Observe,
                permit: None,
                event: "computer.maintenance_cancelled_before_dispatch",
                checkpoint: None,
            },
        )
        .await?;
        self.maintenance_for_claim(claim).await
    }
    /// The owning worker must freshly qualify the exact created run before adoption.
    pub async fn adopt_maintenance(
        &self,
        claim: &ClaimedTask,
        ticket: MaintenanceTicket,
    ) -> Result<MaintenanceOperation> {
        if ticket.is_dispatch()
            || ticket.remaining().is_zero()
            || ticket.operation.stage != MaintenanceStage::Adopting
        {
            return Err(ComputerError::StateConflict);
        }
        let before = self.maintenance_for_claim(claim).await?;
        if before.stage != MaintenanceStage::Adopting || before.created_run().is_none() {
            return Err(ComputerError::InvalidState);
        }
        if before.operation_id != ticket.operation.operation_id
            || before.steps().last().is_none_or(|step| {
                step.dispatch_id != ticket.dispatch_id
                    || step.last_observation_id != ticket.observation_id
            })
        {
            return Err(ComputerError::StateConflict);
        }
        let permit = self.authorize_maintenance(&before).await?;
        let mut after = before.clone();
        after.stage = MaintenanceStage::Succeeded;
        self.commit_maintenance(
            claim,
            &before,
            &after,
            JournalChange {
                kind: ProviderCommit::Dispatch,
                permit: Some(&permit),
                event: "computer.maintenance_adopted",
                checkpoint: None,
            },
        )
        .await?;
        self.maintenance_for_claim(claim).await
    }
}
