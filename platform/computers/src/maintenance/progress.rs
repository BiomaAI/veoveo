//! Bounded private step history; completed dispatch identities are never reused.
use super::{MaintenanceOperation, MaintenanceSource, MaintenanceStage};
use crate::{ComputerError, ExecutionDecision, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceStep {
    Stop,
    Capture,
    Retire,
    Transfer,
    Create,
    Restore,
}
impl MaintenanceStep {
    pub(super) fn stage(self) -> MaintenanceStage {
        match self {
            Self::Stop => MaintenanceStage::Stopping,
            Self::Capture => MaintenanceStage::Capturing,
            Self::Retire => MaintenanceStage::Retiring,
            Self::Transfer => MaintenanceStage::Transferring,
            Self::Create => MaintenanceStage::Creating,
            Self::Restore => MaintenanceStage::Restoring,
        }
    }
}

/// Provider-checked metadata supplied only by the private owning worker.
/// Captured bytes use a separate encrypted row and never enter this projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaintenanceEvidence {
    Stopped {
        resource_id: String,
        process_id: String,
    },
    Captured,
    Retired,
    Transferred,
    Created {
        resource_id: String,
        process_id: String,
    },
    Restored {
        policy_version: u32,
        policy_hash: String,
    },
}
impl MaintenanceEvidence {
    pub(super) fn check(
        &self,
        operation: &MaintenanceOperation,
        step: MaintenanceStep,
    ) -> Result<()> {
        let valid = match (step, self) {
            (
                MaintenanceStep::Stop,
                Self::Stopped {
                    resource_id,
                    process_id,
                },
            ) => {
                matches!(&operation.source, MaintenanceSource::Ready {resource_id: r, process_id: p} if r == resource_id && p == process_id)
            }
            (MaintenanceStep::Capture, Self::Captured) => {
                !matches!(operation.source, MaintenanceSource::InitialFailure { .. })
            }
            (MaintenanceStep::Retire, Self::Retired)
            | (MaintenanceStep::Transfer, Self::Transferred) => true,
            (
                MaintenanceStep::Create,
                Self::Created {
                    resource_id,
                    process_id,
                },
            ) => {
                super::model::native_id(resource_id)
                    && super::model::native_id(process_id)
                    && match &operation.source {
                        MaintenanceSource::Ready { resource_id: r, .. }
                        | MaintenanceSource::Stopped { resource_id: r, .. } => r != resource_id,
                        MaintenanceSource::InitialFailure { .. } => true,
                    }
            }
            (
                MaintenanceStep::Restore,
                Self::Restored {
                    policy_version,
                    policy_hash,
                },
            ) => *policy_version > 0 && super::model::fingerprint(policy_hash),
            _ => false,
        };
        if valid {
            Ok(())
        } else {
            Err(ComputerError::StateConflict)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceRecovery {
    BudgetExhausted,
    AuthorityDenied,
    CancellationRequested,
    InvalidCheckpoint,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceStepRecord {
    pub step: MaintenanceStep,
    pub dispatch_id: Uuid,
    pub authority: ExecutionDecision,
    pub dispatched_at: DateTime<Utc>,
    pub observation_deadline: DateTime<Utc>,
    pub observation_reads: u8,
    pub last_observation_id: Option<Uuid>,
    pub next_observation_at: Option<DateTime<Utc>>,
    pub evidence: Option<MaintenanceEvidence>,
    pub settled_at: Option<DateTime<Utc>>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceProgress {
    pub steps: Vec<MaintenanceStepRecord>,
    pub recovery: Option<MaintenanceRecovery>,
}
impl MaintenanceOperation {
    pub fn steps(&self) -> &[MaintenanceStepRecord] {
        &self.progress.steps
    }
    pub fn recovery(&self) -> Option<MaintenanceRecovery> {
        self.progress.recovery
    }
    pub fn next_step(&self) -> Option<MaintenanceStep> {
        if matches!(
            self.stage,
            MaintenanceStage::Succeeded
                | MaintenanceStage::Cancelled
                | MaintenanceStage::RecoveryRequired
        ) {
            return None;
        }
        let plan = self.step_plan();
        if self
            .progress
            .steps
            .last()
            .is_some_and(|last| last.evidence.is_none())
        {
            return None;
        }
        plan.get(self.progress.steps.len()).copied()
    }
    pub fn created_run(&self) -> Option<(&str, &str)> {
        self.progress
            .steps
            .iter()
            .find_map(|step| match &step.evidence {
                Some(MaintenanceEvidence::Created {
                    resource_id,
                    process_id,
                }) => Some((resource_id.as_str(), process_id.as_str())),
                _ => None,
            })
    }
    fn step_plan(&self) -> &'static [MaintenanceStep] {
        use MaintenanceStep::*;
        match self.source {
            MaintenanceSource::Ready { .. } => &[Stop, Capture, Retire, Transfer, Create, Restore],
            MaintenanceSource::Stopped { .. } => &[Capture, Retire, Transfer, Create, Restore],
            MaintenanceSource::InitialFailure { .. } => &[Transfer, Create],
        }
    }
    pub(super) fn validate_progress(&self) -> Result<()> {
        let fail = || ComputerError::Unavailable;
        let plan = self.step_plan();
        if self.progress.steps.len() > plan.len()
            || (self.stage == MaintenanceStage::RecoveryRequired)
                != self.progress.recovery.is_some()
            || self.task_projected_at.is_some()
                && !matches!(
                    self.stage,
                    MaintenanceStage::Succeeded | MaintenanceStage::Cancelled
                )
        {
            return Err(fail());
        }
        let mut dispatches = std::collections::BTreeSet::new();
        for (index, record) in self.progress.steps.iter().enumerate() {
            let a = &record.authority;
            let accepted = &self.execution_authority;
            if record.step != plan[index]
                || record.dispatch_id.get_version_num() != 7
                || !dispatches.insert(record.dispatch_id)
                || record.observation_deadline <= record.dispatched_at
                || record.observation_deadline - record.dispatched_at
                    > chrono::TimeDelta::seconds(180)
                || record.observation_reads > 8
                || (record.observation_reads == 0) != record.last_observation_id.is_none()
                || (record.observation_reads == 0) != record.next_observation_at.is_none()
                || record
                    .last_observation_id
                    .is_some_and(|id| id.get_version_num() != 7)
                || record.evidence.is_some() != record.settled_at.is_some()
                || record
                    .settled_at
                    .is_some_and(|at| at < record.dispatched_at)
                || record.evidence.is_none() && index + 1 != self.progress.steps.len()
                || a.control_revision.is_empty()
                || !super::model::fingerprint(&a.control_sha256)
                || a.valid_until <= a.checked_at
                || a.valid_until - a.checked_at > chrono::TimeDelta::seconds(5)
                || a.decision.effect != veoveo_mcp_contract::PolicyEffect::Allow
                || a.decision.action != veoveo_mcp_contract::GatewayAction::ToolsCall
                || a.decision.target != super::authority::target()
                || a.decision.profile != accepted.profile
                || a.decision.principal.as_ref() != Some(&accepted.request_context.principal.id)
                || a.decision.tenant.as_ref() != Some(&accepted.invocation.tenant)
                || a.decision.trace_id.as_str() != self.operation_id.to_string()
            {
                return Err(fail());
            }
            if let Some(evidence) = &record.evidence {
                evidence.check(self, record.step).map_err(|_| fail())?;
            }
        }
        let complete = self.progress.steps.len() == plan.len()
            && self
                .progress
                .steps
                .last()
                .is_some_and(|last| last.evidence.is_some());
        let expected = if complete {
            MaintenanceStage::Adopting
        } else {
            self.progress
                .steps
                .last()
                .map(|last| last.step.stage())
                .unwrap_or(MaintenanceStage::Queued)
        };
        if !matches!(
            self.stage,
            MaintenanceStage::RecoveryRequired
                | MaintenanceStage::Cancelled
                | MaintenanceStage::Succeeded
        ) && self.stage != expected
            || self.stage == MaintenanceStage::Succeeded && !complete
            || self.stage == MaintenanceStage::Cancelled && !self.progress.steps.is_empty()
        {
            return Err(fail());
        }
        Ok(())
    }
}
