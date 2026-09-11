use super::{CommandDispatchTicket, CommandOperation};
use crate::{
    ComputerError, ComputersStore, Result,
    api::{ExecutionOutput, ExecutionResult},
    secrets::CommandOutputAccess,
};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

/// An ephemeral receipt held only by the original foreground attempt. Losing it
/// never permits reconstructing an exit code from present Computer state.
pub struct CommandExitTicket {
    dispatch: CommandDispatchTicket,
    exit_code: u8,
    stdout_bytes: u32,
    stderr_bytes: u32,
    publication_deadline: Instant,
}
impl CommandExitTicket {
    pub fn output_access(&self) -> &CommandOutputAccess {
        self.dispatch.output_access()
    }
    pub fn publication_deadline(&self) -> Instant {
        self.publication_deadline
    }
    pub fn operation(&self) -> &CommandOperation {
        self.dispatch.operation()
    }
}
impl CommandDispatchTicket {
    /// The trusted native worker calls this immediately after the qualified
    /// foreground stream returns. It must still enforce current authority around
    /// provider I/O; this receipt proves possession of the original dispatch only.
    pub fn observe_exit(
        self,
        exit_code: i32,
        stdout_bytes: usize,
        stderr_bytes: usize,
    ) -> Result<CommandExitTicket> {
        let now = Instant::now();
        if now >= self.execution_deadline()
            || !(0..=255).contains(&exit_code)
            || exit_code == 124
            || stdout_bytes
                .checked_add(stderr_bytes)
                .is_none_or(|total| total > self.limits().maximum_output_bytes as usize)
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(CommandExitTicket {
            dispatch: self,
            exit_code: exit_code as u8,
            stdout_bytes: stdout_bytes as u32,
            stderr_bytes: stderr_bytes as u32,
            publication_deadline: now
                + Duration::from_secs(u64::from(super::output_access::OUTPUT_PUBLICATION_SECONDS)),
        })
    }
}

pub(super) fn validate_result(result: &ExecutionResult, command: &CommandOperation) -> Result<()> {
    let limits = command.effective_limits.ok_or(ComputerError::Unavailable)?;
    if result.computer_id != command.computer_id()
        || result.execution_id != command.execution_id()
        || result.result_uri.execution_id() != command.execution_id()
        || result.exit_code == 124
        || result.stdout.artifact_id.get_version_num() != 7
        || result.stderr.artifact_id.get_version_num() != 7
        || result.stdout.artifact_id == result.stderr.artifact_id
        || u64::from(result.stdout.byte_count) + u64::from(result.stderr.byte_count)
            > u64::from(limits.maximum_output_bytes)
    {
        return Err(ComputerError::InvalidInput);
    }
    Ok(())
}

impl ComputersStore {
    /// Settle only after both exact output occurrences have been published by the
    /// trusted Artifact client. Publication does not grant permission to read them.
    pub async fn complete_command_output(
        &self,
        claim: &ClaimedTask,
        ticket: CommandExitTicket,
        stdout: ExecutionOutput,
        stderr: ExecutionOutput,
    ) -> Result<CommandOperation> {
        if Instant::now() >= ticket.publication_deadline
            || stdout.byte_count != ticket.stdout_bytes
            || stderr.byte_count != ticket.stderr_bytes
        {
            return Err(ComputerError::StateConflict);
        }
        let mut operation = ticket.dispatch.into_operation();
        let result = ExecutionResult {
            result_uri: crate::api::ExecutionResultUri::new(operation.execution_id())
                .map_err(|_| ComputerError::Unavailable)?,
            computer_id: operation.computer_id(),
            execution_id: operation.execution_id(),
            exit_code: ticket.exit_code,
            stdout,
            stderr,
        };
        validate_result(&result, &operation)?;
        let object = |v: serde_json::Value| -> Result<OpenObject> {
            serde_json::from_value(v).map_err(|_| ComputerError::Unavailable)
        };
        let result_value =
            object(serde_json::to_value(result).map_err(|_| ComputerError::Unavailable)?)?;
        let binding = object(
            serde_json::to_value(&operation.binding).map_err(|_| ComputerError::Unavailable)?,
        )?;
        operation.result = Some(result);
        self.commit_command(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/complete_command_output.surql"),
            vec![
                (
                    "dispatch_id",
                    operation
                        .dispatch_id
                        .ok_or(ComputerError::StateConflict)?
                        .into_value(),
                ),
                ("binding", binding.into_value()),
                ("result", result_value.into_value()),
                (
                    "publication_budget",
                    surrealdb::types::Duration::from_secs(u64::from(
                        super::output_access::OUTPUT_PUBLICATION_SECONDS,
                    ))
                    .into_value(),
                ),
            ],
            "computer.execution_completed",
        )
        .await?;
        self.command_for_claim(claim).await
    }
}
