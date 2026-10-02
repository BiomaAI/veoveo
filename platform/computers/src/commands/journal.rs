use super::{CommandOperation, model};
use crate::{ComputerError, ComputersStore, Result};
use chrono::{DateTime, Utc};
use std::time::Instant;
use surrealdb::types::{SurrealValue, Value};
use veoveo_task_runtime::{ClaimedTask, ProviderCommit, TaskError, TaskRuntime};

pub(super) struct ClockedCommand {
    pub operation: CommandOperation,
    pub database_time: DateTime<Utc>,
    pub read_started: Instant,
}
impl ClockedCommand {
    pub fn remaining(&self, until: Option<DateTime<Utc>>) -> std::time::Duration {
        until
            .and_then(|deadline| (deadline - self.database_time).to_std().ok())
            .unwrap_or_default()
            .saturating_sub(self.read_started.elapsed())
    }
}
impl ComputersStore {
    pub async fn command_for_claim(&self, claim: &ClaimedTask) -> Result<CommandOperation> {
        Ok(self.worker_command(claim).await?.operation)
    }
    pub(super) async fn worker_command(&self, claim: &ClaimedTask) -> Result<ClockedCommand> {
        let reference: crate::task_references::CommandReference =
            serde_json::from_value(claim.snapshot.request.clone())
                .map_err(|_| ComputerError::StateConflict)?;
        let id = reference.execution_id;
        let mut params = crate::task_references::worker_bindings(
            claim,
            crate::api::ComputerTaskKind::Execution,
            reference.computer_id,
            reference.execution_id.task_id(),
        )?;
        params.extend([
            ("journal", super::record(id).into_value()),
            ("provider", self.provider_instance_id.into_value()),
        ]);
        let started = Instant::now();
        let mut read = self
            .query(include_str!("../../queries/worker_execution.surql"), params)
            .await?;
        let row: Option<model::Record> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let database_time: Option<DateTime<Utc>> =
            read.take(1).map_err(|_| ComputerError::Unavailable)?;
        let operation = CommandOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        Ok(ClockedCommand {
            operation,
            database_time: database_time.ok_or(ComputerError::Unavailable)?,
            read_started: started,
        })
    }
    pub(super) async fn commit_command(
        &self,
        claim: &ClaimedTask,
        operation: &CommandOperation,
        kind: ProviderCommit,
        body: &'static str,
        mut params: Vec<(&'static str, Value)>,
        event: crate::audit::ExecutionTransition,
    ) -> Result<()> {
        params.extend([
            (
                "execution",
                super::record(operation.execution_id()).into_value(),
            ),
            (
                "execution_id",
                operation.execution_id().into_uuid().into_value(),
            ),
            (
                "computer",
                crate::model::computer_record(operation.computer_id()).into_value(),
            ),
            ("slot", super::slot(operation.computer_id()).into_value()),
            ("provider", self.provider_instance_id.into_value()),
        ]);
        use veoveo_audit_contract::AuditReason;
        let failure = match event {
            crate::audit::ExecutionTransition::Undispatched => {
                operation.refusal.map(|reason| match reason {
                    super::CommandRefusal::CancelledBeforeDispatch => AuditReason::Cancelled,
                    super::CommandRefusal::AuthorityDenied => AuditReason::PolicyDenied,
                    super::CommandRefusal::RunChanged => AuditReason::Conflict,
                    super::CommandRefusal::PreparationExpired => AuditReason::TimedOut,
                })
            }
            crate::audit::ExecutionTransition::Terminated => {
                operation.interruption.map(|reason| match reason {
                    super::CommandInterruption::Cancelled => AuditReason::Cancelled,
                    super::CommandInterruption::Deadline => AuditReason::TimedOut,
                    super::CommandInterruption::AuthorityLost => AuditReason::Revoked,
                    super::CommandInterruption::ExecutionUnknown => AuditReason::Unavailable,
                })
            }
            crate::audit::ExecutionTransition::Completed => operation
                .result
                .filter(|result| result.exit_code != 0)
                .map(|_| AuditReason::UpstreamFailure),
            _ => None,
        };
        params.push(crate::audit::binding(
            &operation.authority,
            operation.computer_id(),
            event.transition(
                crate::audit::ExecutionDomain::Command,
                operation.task_id(),
                failure,
            )?,
        )?);
        TaskRuntime::new(self.platform.clone(), "computers", &claim.lease_owner)
            .commit_provider_journal(claim, kind, body, params)
            .await
            .map_err(|error| match error {
                TaskError::Conflict(_) | TaskError::LeaseHeld(_) => ComputerError::StateConflict,
                _ => ComputerError::Unavailable,
            })
    }
}
