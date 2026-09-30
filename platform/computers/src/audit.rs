//! Domain transition records use the accepted actor and join the state transaction.
use crate::{
    AcceptedAuthority, ComputerError, Result,
    api::{Action, ComputerId},
};
use surrealdb::types::Value;
use veoveo_audit_contract::*;
use veoveo_platform_store::audit::AuditTransactionWrite;

pub(crate) fn lifecycle_activity(action: Action) -> ComputerActivity {
    match action {
        Action::Create => ComputerActivity::Create,
        Action::Start => ComputerActivity::Start,
        Action::Stop => ComputerActivity::Stop,
    }
}

pub(crate) struct Transition {
    pub activity: ComputerActivity,
    pub stage: ComputerAuditStage,
    pub task: Option<veoveo_types::TaskId>,
    pub outcome: AuditOutcome,
    pub reason: AuditReason,
}
impl Transition {
    pub fn accepted(activity: ComputerActivity, stage: ComputerAuditStage) -> Self {
        Self {
            activity,
            stage,
            task: None,
            outcome: AuditOutcome::Succeeded,
            reason: AuditReason::Accepted,
        }
    }
    pub fn task(mut self, task: veoveo_types::TaskId) -> Self {
        self.task = Some(task);
        self
    }
}
pub(crate) fn binding(
    authority: &AcceptedAuthority,
    computer: ComputerId,
    transition: Transition,
) -> Result<(&'static str, Value)> {
    let context = authority
        .request_context
        .audit_context(&authority.actor, &authority.invocation, &authority.profile)
        .map_err(|_| ComputerError::Forbidden)?;
    let draft = context
        .draft(
            AuditTarget::Computer { computer },
            AuditDetail::Computer {
                activity: transition.activity,
                stage: transition.stage,
                task: transition.task,
            },
            transition.outcome,
            transition.reason,
        )
        .map_err(|_| ComputerError::InvalidInput)?;
    AuditTransactionWrite::new(draft)
        .map(AuditTransactionWrite::into_binding)
        .map_err(|_| ComputerError::Unavailable)
}

#[derive(Clone, Copy)]
pub(crate) enum LifecycleTransition {
    Dispatch,
    RecoveryRequired,
    Settle,
    Abort(crate::UndispatchedOutcome),
}
impl LifecycleTransition {
    pub fn event(self) -> &'static str {
        match self {
            Self::Dispatch => "computer.operation_dispatched",
            Self::RecoveryRequired => "computer.recovery_required",
            Self::Settle => "computer.operation_succeeded",
            Self::Abort(_) => "computer.operation_undispatched",
        }
    }
    pub fn transition(self, operation: &crate::Operation) -> Transition {
        let mut transition = Transition::accepted(
            lifecycle_activity(operation.action),
            match self {
                Self::Dispatch => ComputerAuditStage::Dispatched,
                Self::RecoveryRequired => ComputerAuditStage::RecoveryRequired,
                Self::Settle => ComputerAuditStage::Settled,
                Self::Abort(_) => ComputerAuditStage::Aborted,
            },
        )
        .task(operation.task_id());
        if let Self::Abort(reason) = self {
            transition.outcome = AuditOutcome::Denied;
            transition.reason = match reason {
                crate::UndispatchedOutcome::AuthorityDenied => AuditReason::PolicyDenied,
                crate::UndispatchedOutcome::CancelledBeforeDispatch => AuditReason::Cancelled,
            };
        }
        transition
    }
}

mod execution;
pub(crate) use execution::{ExecutionDomain, ExecutionTransition};

pub(crate) fn maintenance(operation: &crate::maintenance::MaintenanceOperation) -> Transition {
    use crate::maintenance::MaintenanceStage;
    let stage = match operation.stage {
        MaintenanceStage::Succeeded => ComputerAuditStage::Settled,
        MaintenanceStage::Cancelled => ComputerAuditStage::Aborted,
        MaintenanceStage::RecoveryRequired => ComputerAuditStage::RecoveryRequired,
        MaintenanceStage::Queued => ComputerAuditStage::Queued,
        MaintenanceStage::Stopping
        | MaintenanceStage::Capturing
        | MaintenanceStage::Retiring
        | MaintenanceStage::Transferring
        | MaintenanceStage::Creating
        | MaintenanceStage::Restoring
        | MaintenanceStage::Adopting => ComputerAuditStage::MaintenanceProgress,
    };
    let mut transition =
        Transition::accepted(ComputerActivity::Maintain, stage).task(operation.task_id());
    if operation.stage == MaintenanceStage::Cancelled {
        transition.outcome = AuditOutcome::Denied;
        transition.reason = AuditReason::Cancelled;
    }
    transition
}
