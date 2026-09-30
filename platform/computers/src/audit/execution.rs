//! Closed journal transitions shared by command and file execution.
use super::Transition;
use veoveo_audit_contract::{AuditOutcome, AuditReason, ComputerActivity, ComputerAuditStage};

#[derive(Clone, Copy)]
pub(crate) enum ExecutionDomain {
    Command,
    File,
}
#[derive(Clone, Copy)]
pub(crate) enum ExecutionTransition {
    Dispatch,
    Undispatched,
    Terminated,
    Completed,
    ContainmentRequested,
    StopDispatched,
    RecoveryRequired,
}
impl ExecutionTransition {
    pub fn event(self, domain: ExecutionDomain) -> String {
        let prefix = match domain {
            ExecutionDomain::Command => "execution",
            ExecutionDomain::File => "file_transfer",
        };
        let suffix = match self {
            Self::Dispatch => "dispatched",
            Self::Undispatched => "undispatched",
            Self::Terminated => "terminated",
            Self::Completed => match domain {
                ExecutionDomain::Command => "completed",
                ExecutionDomain::File => "finished",
            },
            Self::ContainmentRequested => "containment_requested",
            Self::StopDispatched => "stop_dispatched",
            Self::RecoveryRequired => "recovery_required",
        };
        format!("computer.{prefix}_{suffix}")
    }
    pub fn transition(
        self,
        domain: ExecutionDomain,
        task: veoveo_types::TaskId,
        failure: Option<AuditReason>,
    ) -> crate::Result<Transition> {
        let activity = match domain {
            ExecutionDomain::Command => ComputerActivity::Command,
            ExecutionDomain::File => ComputerActivity::FileTransfer,
        };
        let stage = match self {
            Self::Dispatch => ComputerAuditStage::Dispatched,
            Self::Undispatched => ComputerAuditStage::Aborted,
            Self::Terminated => ComputerAuditStage::Terminated,
            Self::Completed => ComputerAuditStage::Settled,
            Self::ContainmentRequested => ComputerAuditStage::ContainmentRequested,
            Self::StopDispatched => ComputerAuditStage::StopDispatched,
            Self::RecoveryRequired => ComputerAuditStage::RecoveryRequired,
        };
        let mut transition = Transition::accepted(activity, stage).task(task);
        if matches!(self, Self::Undispatched | Self::Terminated) && failure.is_none() {
            return Err(crate::ComputerError::InvalidState);
        }
        if let Some(reason) = failure {
            transition.outcome = if matches!(self, Self::Undispatched) {
                AuditOutcome::Denied
            } else {
                AuditOutcome::Failed
            };
            transition.reason = reason;
        }
        Ok(transition)
    }
}
