use super::{CommandOperation, model};
use crate::{ComputerError, ComputersStore, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::time::Instant;
use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_platform_store::{OutboxDraft, deterministic_tenant_id};
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
        if claim.snapshot.server != "computers" || claim.snapshot.task_type != "computer.execution"
        {
            return Err(ComputerError::InvalidInput);
        }
        let id = Uuid::parse_str(&claim.snapshot.task_id.to_string())
            .map_err(|_| ComputerError::InvalidInput)?;
        let started = Instant::now();
        let mut read = self
            .query(
                "SELECT * FROM ONLY $execution; RETURN time::now();",
                vec![("execution", super::record(id).into_value())],
            )
            .await?;
        let row: Option<model::Record> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let database_time: Option<DateTime<Utc>> =
            read.take(1).map_err(|_| ComputerError::Unavailable)?;
        let operation = CommandOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        if operation.task_id() != claim.snapshot.task_id
            || operation.actor() != claim.snapshot.owner
            || operation.binding.provider_instance_id != self.provider_instance_id
            || operation.task_reference()? != claim.snapshot.request
        {
            return Err(ComputerError::StateConflict);
        }
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
        event: &str,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Event<'a> {
            execution_id: Uuid,
            computer_id: Uuid,
            grant_id: Uuid,
            actor: &'a crate::AcceptedAuthority,
            dispatch_authority: Option<&'a super::CommandDispatchDecision>,
            interruption: Option<super::CommandInterruption>,
            refusal: Option<super::CommandRefusal>,
            termination_evidence: Option<super::outcome::TerminationEvidence>,
        }
        let payload = serde_json::from_value(
            serde_json::to_value(Event {
                execution_id: operation.execution_id(),
                computer_id: operation.computer_id(),
                grant_id: operation.binding.grant_id,
                actor: &operation.authority,
                dispatch_authority: operation.dispatch_authority.as_ref(),
                interruption: operation.interruption,
                refusal: operation.refusal,
                termination_evidence: operation.termination_evidence,
            })
            .map_err(|_| ComputerError::Unavailable)?,
        )
        .map_err(|_| ComputerError::Unavailable)?;
        let outbox = OutboxDraft::now(
            Some(
                deterministic_tenant_id(operation.actor().tenant_key())
                    .map_err(|_| ComputerError::Unavailable)?
                    .record_id(),
            ),
            "computer",
            operation.computer_id().to_string(),
            event,
            1,
            payload,
        );
        params.extend([
            (
                "execution",
                super::record(operation.execution_id()).into_value(),
            ),
            ("execution_id", operation.execution_id().into_value()),
            (
                "computer",
                crate::model::computer_record(operation.computer_id()).into_value(),
            ),
            ("slot", super::slot(operation.computer_id()).into_value()),
            ("provider", self.provider_instance_id.into_value()),
            ("event", outbox.into_value()),
        ]);
        TaskRuntime::new(self.platform.clone(), "computers", &claim.lease_owner)
            .commit_provider_journal(claim, kind, body, params)
            .await
            .map_err(|error| match error {
                TaskError::Conflict(_) | TaskError::LeaseHeld(_) => ComputerError::StateConflict,
                _ => ComputerError::Unavailable,
            })
    }
}
