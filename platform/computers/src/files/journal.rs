use super::{FileOperation, model};
use crate::{ComputerError, ComputersStore, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::time::Instant;
use surrealdb::types::{SurrealValue, Value};
use veoveo_platform_store::{OutboxDraft, deterministic_tenant_id};
use veoveo_task_runtime::{ClaimedTask, ProviderCommit, TaskError, TaskRuntime};
use veoveo_types::TaskTypeDefinition;

pub(super) struct ClockedFile {
    pub operation: FileOperation,
    pub database_time: DateTime<Utc>,
    pub read_started: Instant,
}
impl ClockedFile {
    pub fn remaining(&self, until: Option<DateTime<Utc>>) -> std::time::Duration {
        until
            .and_then(|deadline| (deadline - self.database_time).to_std().ok())
            .unwrap_or_default()
            .saturating_sub(self.read_started.elapsed())
    }
}
impl ComputersStore {
    pub async fn file_for_claim(&self, claim: &ClaimedTask) -> Result<FileOperation> {
        Ok(self.worker_file(claim).await?.operation)
    }
    pub(super) async fn worker_file(&self, claim: &ClaimedTask) -> Result<ClockedFile> {
        if claim.snapshot.server != "computers"
            || claim.snapshot.task_type != crate::api::ComputerTaskKind::FileTransfer.name()
        {
            return Err(ComputerError::InvalidInput);
        }
        let id = crate::api::FileTransferId::try_from(claim.snapshot.task_id.as_uuid())
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
        let operation = FileOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        if operation.task_id() != claim.snapshot.task_id
            || operation.actor() != claim.snapshot.owner
            || operation.binding.provider_instance_id != self.provider_instance_id
            || operation.task_reference()? != claim.snapshot.request
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(ClockedFile {
            operation,
            database_time: database_time.ok_or(ComputerError::Unavailable)?,
            read_started: started,
        })
    }
    pub(super) async fn commit_file(
        &self,
        claim: &ClaimedTask,
        operation: &FileOperation,
        kind: ProviderCommit,
        body: &'static str,
        mut params: Vec<(&'static str, Value)>,
        event: crate::audit::ExecutionTransition,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Event<'a> {
            transfer_id: veoveo_computers_contract::FileTransferId,
            computer_id: veoveo_computers_contract::ComputerId,
            grant_id: Option<veoveo_computers_contract::AutomationGrantId>,
            actor: &'a crate::AcceptedAuthority,
            dispatch_authority: Option<&'a super::FileDispatchDecision>,
            interruption: Option<super::FileInterruption>,
            refusal: Option<super::FileRefusal>,
            termination_evidence: Option<super::outcome::TerminationEvidence>,
            result: Option<crate::api::FileTransferResult>,
            rejection: Option<veoveo_computer_execution::FileFailure>,
        }
        let payload = serde_json::from_value(
            serde_json::to_value(Event {
                transfer_id: operation.transfer_id(),
                computer_id: operation.computer_id(),
                grant_id: operation.binding.grant_id,
                actor: &operation.authority,
                dispatch_authority: operation.dispatch_authority.as_ref(),
                interruption: operation.interruption,
                refusal: operation.refusal,
                termination_evidence: operation.termination_evidence,
                result: operation.result.clone(),
                rejection: operation.rejection,
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
            event.event(crate::audit::ExecutionDomain::File),
            1,
            payload,
        );
        params.extend([
            (
                "execution",
                super::record(operation.transfer_id()).into_value(),
            ),
            (
                "transfer_id",
                operation.transfer_id().into_uuid().into_value(),
            ),
            (
                "computer",
                crate::model::computer_record(operation.computer_id()).into_value(),
            ),
            (
                "slot",
                crate::commands::slot(operation.computer_id()).into_value(),
            ),
            ("provider", self.provider_instance_id.into_value()),
            ("event", outbox.into_value()),
        ]);
        use veoveo_audit_contract::AuditReason;
        let failure = match event {
            crate::audit::ExecutionTransition::Undispatched => {
                operation.refusal.map(|reason| match reason {
                    super::FileRefusal::CancelledBeforeDispatch => AuditReason::Cancelled,
                    super::FileRefusal::AuthorityDenied => AuditReason::PolicyDenied,
                    super::FileRefusal::RunChanged => AuditReason::Conflict,
                    super::FileRefusal::PreparationExpired => AuditReason::TimedOut,
                    super::FileRefusal::ArtifactUnavailable => AuditReason::Unavailable,
                })
            }
            crate::audit::ExecutionTransition::Terminated => {
                operation.interruption.map(|reason| match reason {
                    super::FileInterruption::Cancelled => AuditReason::Cancelled,
                    super::FileInterruption::Deadline => AuditReason::TimedOut,
                    super::FileInterruption::AuthorityLost => AuditReason::Revoked,
                    super::FileInterruption::ExecutionUnknown => AuditReason::Unavailable,
                })
            }
            crate::audit::ExecutionTransition::Completed => {
                operation.rejection.map(|_| AuditReason::UpstreamFailure)
            }
            _ => None,
        };
        params.push(crate::audit::binding(
            &operation.authority,
            operation.computer_id(),
            event.transition(
                crate::audit::ExecutionDomain::File,
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
