use super::{FileContinuation, FileOperation, FileRunAuthority};
use crate::{
    ComputerError, ComputersStore, Result,
    api::FileTransferStage,
    secrets::{ComputerKeyRing, FileTransferAccess, FileTransferPayload},
};
use veoveo_task_runtime::ClaimedTask;

/// Private, currently authorized preparation. This grants no native dispatch.
pub struct FilePreparation {
    pub operation: FileOperation,
    pub payload: FileTransferPayload,
    pub access: FileTransferAccess,
    pub authority: FileRunAuthority,
    pub retained_labels: std::collections::BTreeSet<veoveo_mcp_contract::DataLabelId>,
}
impl ComputersStore {
    pub async fn prepare_file_transfer(
        &self,
        claim: &ClaimedTask,
        keys: &ComputerKeyRing,
    ) -> Result<FilePreparation> {
        let operation = self.worker_file(claim).await?.operation;
        if operation.stage != FileTransferStage::Queued {
            return Err(ComputerError::StateConflict);
        }
        let payload = keys.open_file_transfer(&operation.binding, &operation.sealed)?;
        let retained_labels = self
            .accepted_file_authority(&operation)
            .await?
            .retained_labels()?;
        let access = keys.open_file_access(
            &operation.binding,
            operation
                .access
                .as_ref()
                .ok_or(ComputerError::InvalidState)?,
        )?;
        let FileContinuation::Authorized(mut authority) =
            self.file_continuation(claim, &operation).await?
        else {
            return Err(ComputerError::Forbidden);
        };
        authority.maximum_bytes = authority.maximum_bytes.min(payload.limits().maximum_bytes);
        Ok(FilePreparation {
            operation,
            payload,
            access,
            authority,
            retained_labels,
        })
    }
}
