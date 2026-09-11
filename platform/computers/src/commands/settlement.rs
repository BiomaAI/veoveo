use super::outcome::TerminationSource;
use super::{CommandContainmentRead, CommandContainmentStop, CommandOperation, CommandRefusal};
use crate::{ComputerError, ComputersStore, ReachedPhase, ReachedState, Result};
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

impl ComputersStore {
    pub async fn abort_queued_command(
        &self,
        claim: &ClaimedTask,
        reason: CommandRefusal,
    ) -> Result<CommandOperation> {
        let mut operation = self.worker_command(claim).await?.operation;
        operation.refusal = Some(reason);
        self.commit_command(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/abort_queued_command.surql"),
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
                        super::output_access::PREPARATION_ALLOWANCE_SECONDS,
                    ))
                    .into_value(),
                ),
            ],
            "computer.execution_undispatched",
        )
        .await?;
        self.command_for_claim(claim).await
    }
    pub async fn complete_command_stop(
        &self,
        claim: &ClaimedTask,
        ticket: CommandContainmentStop,
        stopped: ReachedState,
    ) -> Result<CommandOperation> {
        self.settle_command_containment(
            claim,
            *ticket.operation,
            TerminationSource::Stop,
            ticket.id,
            stopped,
        )
        .await
    }
    pub async fn complete_command_containment_read(
        &self,
        claim: &ClaimedTask,
        ticket: CommandContainmentRead,
        stopped: ReachedState,
    ) -> Result<CommandOperation> {
        self.settle_command_containment(
            claim,
            *ticket.operation,
            TerminationSource::Read,
            ticket.id,
            stopped,
        )
        .await
    }
    async fn settle_command_containment(
        &self,
        claim: &ClaimedTask,
        mut operation: CommandOperation,
        kind: TerminationSource,
        id: Uuid,
        stopped: ReachedState,
    ) -> Result<CommandOperation> {
        let binding = &operation.binding;
        if stopped.provider_instance_id != binding.provider_instance_id
            || stopped.replacement_instance_id != binding.replacement_instance_id
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
        self.commit_command(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/settle_command_containment.surql"),
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
            "computer.execution_terminated",
        )
        .await?;
        self.command_for_claim(claim).await
    }
}
