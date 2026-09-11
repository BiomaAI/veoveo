use super::outcome::TerminationSource;
use super::{FileContainmentRead, FileContainmentStop, FileOperation, FileRefusal};
use crate::{ComputerError, ComputersStore, ReachedPhase, ReachedState, Result};
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

impl ComputersStore {
    pub async fn abort_queued_file(
        &self,
        claim: &ClaimedTask,
        reason: FileRefusal,
    ) -> Result<FileOperation> {
        let mut operation = self.worker_file(claim).await?.operation;
        operation.refusal = Some(reason);
        self.commit_file(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/abort_queued_file.surql"),
            vec![
                (
                    "refusal",
                    serde_json::to_value(reason)
                        .map_err(|_| ComputerError::Unavailable)?
                        .as_str()
                        .ok_or(ComputerError::Unavailable)?
                        .to_owned()
                        .into_value(),
                ),
                (
                    "preparation_budget",
                    surrealdb::types::Duration::from_secs(u64::from(
                        super::FILE_PREPARATION_SECONDS,
                    ))
                    .into_value(),
                ),
            ],
            "computer.file_transfer_undispatched",
        )
        .await?;
        self.file_for_claim(claim).await
    }
    pub async fn complete_file_stop(
        &self,
        claim: &ClaimedTask,
        ticket: FileContainmentStop,
        stopped: ReachedState,
    ) -> Result<FileOperation> {
        self.settle_file_containment(
            claim,
            *ticket.operation,
            TerminationSource::Stop,
            ticket.id,
            stopped,
        )
        .await
    }
    pub async fn complete_file_containment_read(
        &self,
        claim: &ClaimedTask,
        ticket: FileContainmentRead,
        stopped: ReachedState,
    ) -> Result<FileOperation> {
        self.settle_file_containment(
            claim,
            *ticket.operation,
            TerminationSource::Read,
            ticket.id,
            stopped,
        )
        .await
    }
    async fn settle_file_containment(
        &self,
        claim: &ClaimedTask,
        mut operation: FileOperation,
        kind: TerminationSource,
        id: Uuid,
        stopped: ReachedState,
    ) -> Result<FileOperation> {
        let binding = &operation.binding;
        if stopped.provider_instance_id != binding.provider_instance_id
            || stopped
                .replacement_instance_id
                .unwrap_or(stopped.computer_id)
                != binding.instance_id
            || stopped.computer_id != binding.computer_id
            || stopped.template_fingerprint != binding.template_fingerprint
            || stopped.resource_id != binding.resource_id
            || stopped.process_id != binding.process_id
            || stopped.phase != ReachedPhase::Stopped
        {
            return Err(ComputerError::StateConflict);
        }
        operation.termination_evidence = Some(super::outcome::TerminationEvidence { kind, id });
        let source = match kind {
            TerminationSource::Stop => "stop",
            TerminationSource::Read => "read",
        };
        self.commit_file(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/settle_file_containment.surql"),
            vec![
                (
                    "containment_id",
                    operation
                        .containment_id
                        .ok_or(ComputerError::StateConflict)?
                        .into_value(),
                ),
                ("evidence_kind", source.into_value()),
                ("evidence_id", id.into_value()),
                ("resource", stopped.resource_id.into_value()),
                ("process", stopped.process_id.into_value()),
            ],
            "computer.file_transfer_terminated",
        )
        .await?;
        self.file_for_claim(claim).await
    }
}
