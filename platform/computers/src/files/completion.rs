use super::{FileDispatchTicket, FileOperation};
use crate::{
    ComputerError, ComputersStore, Result,
    api::{FileTransfer, FileTransferResult},
    secrets::FileTransferAccess,
};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_computer_execution::{FileFailure, FileReceipt};
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

/// Possession of the original dispatch and its verified native result. A new
/// worker cannot infer success from the current file or Computer state.
pub struct FileExitTicket {
    dispatch: FileDispatchTicket,
    result: std::result::Result<FileReceipt, FileFailure>,
    publication_deadline: Instant,
}
impl FileExitTicket {
    pub fn operation(&self) -> &FileOperation {
        self.dispatch.operation()
    }
    pub fn access(&self) -> &FileTransferAccess {
        self.dispatch.access()
    }
    pub fn publication_deadline(&self) -> Instant {
        self.publication_deadline
    }
    pub fn receipt(&self) -> std::result::Result<&FileReceipt, FileFailure> {
        self.result.as_ref().map_err(|reason| *reason)
    }
}
impl FileDispatchTicket {
    /// The trusted worker calls this only after the native adapter verifies its
    /// framed receipt, byte count, hash and unchanged provider process.
    pub fn observe_file_result(
        self,
        result: std::result::Result<FileReceipt, FileFailure>,
    ) -> Result<FileExitTicket> {
        let now = Instant::now();
        if now >= self.execution_deadline()
            || result
                .as_ref()
                .is_ok_and(|receipt| receipt.bytes > self.limits().maximum_bytes)
            || result == Err(FileFailure::CommitUnknown)
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(FileExitTicket {
            dispatch: self,
            result,
            publication_deadline: now
                + Duration::from_secs(u64::from(super::FILE_PUBLICATION_SECONDS)),
        })
    }
}

pub(super) fn validate_result(
    result: &FileTransferResult,
    operation: &FileOperation,
) -> Result<()> {
    if result.transfer_id != operation.transfer_id()
        || result.computer_id != operation.computer_id()
        || result.result_uri != crate::api::file_transfer_uri(operation.transfer_id())
        || result.direction != operation.binding.direction
        || result.artifact_id.get_version_num() != 7
        || result.bytes
            > operation
                .effective_limits
                .ok_or(ComputerError::Unavailable)?
                .maximum_bytes
        || result.sha256.len() != 64
        || !result
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ComputerError::InvalidInput);
    }
    Ok(())
}

impl ComputersStore {
    /// Exports settle after one exact governed Artifact occurrence is verified.
    /// Imports refer to the authenticated source occurrence; a rejection has none.
    pub async fn complete_file_result(
        &self,
        claim: &ClaimedTask,
        ticket: FileExitTicket,
        artifact: Option<Uuid>,
    ) -> Result<FileOperation> {
        if Instant::now() >= ticket.publication_deadline {
            return Err(ComputerError::StateConflict);
        }
        if let (Ok(_), FileTransfer::Import { artifact_id, .. }) =
            (&ticket.result, ticket.dispatch.payload().transfer())
            && artifact != Some(*artifact_id)
        {
            return Err(ComputerError::InvalidInput);
        }
        let mut operation = ticket.dispatch.into_operation();
        let (result, rejection) = match ticket.result {
            Ok(receipt) => {
                let result = FileTransferResult {
                    result_uri: crate::api::file_transfer_uri(operation.transfer_id()),
                    computer_id: operation.computer_id(),
                    transfer_id: operation.transfer_id(),
                    direction: operation.binding.direction,
                    artifact_id: artifact.ok_or(ComputerError::InvalidInput)?,
                    bytes: receipt.bytes,
                    sha256: hex::encode(receipt.sha256),
                };
                validate_result(&result, &operation)?;
                (Some(result), None)
            }
            Err(reason) => {
                if artifact.is_some() || reason == FileFailure::CommitUnknown {
                    return Err(ComputerError::InvalidInput);
                }
                (None, Some(reason))
            }
        };
        let rejection_value = rejection
            .map(|reason| {
                serde_json::to_value(reason)
                    .map_err(|_| ComputerError::Unavailable)?
                    .as_str()
                    .map(str::to_owned)
                    .ok_or(ComputerError::Unavailable)
            })
            .transpose()?;
        let result_value = result.as_ref().map(super::object).transpose()?;
        operation.result = result;
        operation.rejection = rejection;
        self.commit_file(
            claim,
            &operation,
            ProviderCommit::Observe,
            include_str!("../../queries/complete_file_result.surql"),
            vec![
                (
                    "dispatch_id",
                    operation
                        .dispatch_id
                        .ok_or(ComputerError::StateConflict)?
                        .into_value(),
                ),
                ("binding", super::object(&operation.binding)?.into_value()),
                ("result", result_value.into_value()),
                ("rejection", rejection_value.into_value()),
                (
                    "publication_budget",
                    surrealdb::types::Duration::from_secs(u64::from(
                        super::FILE_PUBLICATION_SECONDS,
                    ))
                    .into_value(),
                ),
            ],
            "computer.file_transfer_finished",
        )
        .await?;
        self.file_for_claim(claim).await
    }
}
