use super::{Application, ApplicationError, Result};
use std::time::Instant;
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputerError,
    api::{FileTransferResult, FileTransferStage, FileTransferView},
    files::{FileTaskAccess, FileTaskAction},
};
use veoveo_task_runtime::{TaskSnapshot, TaskStatus};

impl Application {
    pub async fn file_transfer(
        &self,
        actor: &ComputerActor,
        computer: Uuid,
        transfer: Uuid,
        cancel: bool,
    ) -> Result<FileTransferView> {
        let action = if cancel {
            FileTaskAction::Cancel
        } else {
            FileTaskAction::Observe
        };
        let access = self
            .store
            .authorize_file_task(actor, transfer, action)
            .await?;
        if access.computer_id() != computer {
            return Err(ComputerError::NotFound.into());
        }
        let read = async {
            if cancel {
                veoveo_task_runtime::cancel_durable_task(
                    &self.tasks,
                    access.owner()?,
                    transfer.to_string(),
                )
                .await
                .map_err(|_| ApplicationError::Unavailable)?;
            }
            let task = self.file_snapshot(&access).await?;
            let result = result(&access, &task)?;
            let can_cancel = access.can_cancel()
                && task.cancel_requested_at.is_none()
                && !matches!(
                    access.stage(),
                    FileTransferStage::Completed
                        | FileTransferStage::Failed
                        | FileTransferStage::Cancelled
                        | FileTransferStage::RecoveryRequired
                );
            let message = task
                .error
                .as_ref()
                .map(|e| e.message.clone())
                .or(task.status_message);
            if message.as_ref().is_some_and(|v| v.len() > 4096) {
                return Err(ApplicationError::Unavailable);
            }
            access.owner()?;
            Ok(FileTransferView {
                task_id: transfer,
                computer_id: computer,
                direction: access.direction(),
                stage: access.stage(),
                message,
                cancellation_requested_at: task.cancel_requested_at,
                can_cancel,
                result,
                created_at: task.created_at,
                updated_at: task.updated_at,
                completed_at: task.completed_at,
            })
        };
        tokio::time::timeout_at(tokio::time::Instant::from_std(access.valid_until()), read)
            .await
            .map_err(|_| ComputerError::Forbidden)?
    }

    pub async fn file_result(
        &self,
        actor: &ComputerActor,
        transfer: Uuid,
    ) -> Result<FileTransferResult> {
        let access = self
            .store
            .authorize_file_task(actor, transfer, FileTaskAction::Observe)
            .await?;
        let task = self.file_snapshot(&access).await?;
        let result = result(&access, &task)?.ok_or(ComputerError::InvalidState)?;
        access.owner()?;
        Ok(result)
    }

    async fn file_snapshot(&self, access: &FileTaskAccess) -> Result<TaskSnapshot> {
        let task = tokio::time::timeout_at(
            tokio::time::Instant::from_std(access.valid_until()),
            veoveo_task_runtime::authorized_snapshot(
                &self.tasks,
                access.owner()?,
                &access.transfer_id().to_string(),
            ),
        )
        .await
        .map_err(|_| ComputerError::Forbidden)?
        .map_err(|_| ApplicationError::Unavailable)?;
        if task.task_type != "computer.file_transfer" || Instant::now() >= access.valid_until() {
            return Err(ApplicationError::Unavailable);
        }
        access.owner()?;
        Ok(task)
    }
}

fn result(access: &FileTaskAccess, task: &TaskSnapshot) -> Result<Option<FileTransferResult>> {
    if task.status != TaskStatus::Succeeded {
        return Ok(None);
    }
    let response: rmcp::model::CallToolResult =
        serde_json::from_value(task.result.clone().ok_or(ApplicationError::Unavailable)?)
            .map_err(|_| ApplicationError::Unavailable)?;
    let result: FileTransferResult = serde_json::from_value(
        response
            .structured_content
            .ok_or(ApplicationError::Unavailable)?,
    )
    .map_err(|_| ApplicationError::Unavailable)?;
    if response.is_error == Some(true)
        || access.stage() != FileTransferStage::Completed
        || result.transfer_id != access.transfer_id()
        || result.result_uri.transfer_id() != access.transfer_id()
        || result.computer_id != access.computer_id()
        || result.direction != access.direction()
        || result.artifact_id.get_version_num() != 7
        || result.bytes > veoveo_computers::api::MAX_TRANSFER_BYTES
        || result.sha256.len() != 64
        || !result
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ApplicationError::Unavailable);
    }
    Ok(Some(result))
}
