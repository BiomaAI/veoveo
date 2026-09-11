//! Actual-caller Artifact preparation and metadata-only public file operations.
use super::{Application, ApplicationError, Result, execution::ExecutionSupport};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_computers::{
    ComputerActor,
    api::{FileTransferStage, TransferFileInput},
    files::{FileCapabilityRequest, FileOperation, FileTaskAction},
    secrets::{ComputerKeyRing, FileTransferAccess, FileTransferPayload},
};
use veoveo_mcp_contract::PlaneCaller;

impl Application {
    /// Exact installation-qualified templates; admission never infers helper
    /// support from an executable version or a mutable image name.
    pub fn with_files(
        mut self,
        keys: Arc<ComputerKeyRing>,
        artifacts: HttpArtifactPlane,
        templates: BTreeSet<String>,
    ) -> Result<Self> {
        let admitted: BTreeSet<_> = self
            .templates
            .runtimes()
            .iter()
            .map(|t| t.fingerprint())
            .collect();
        if templates.is_empty() || templates.len() > 64 || !templates.is_subset(&admitted) {
            return Err(ApplicationError::Configuration);
        }
        self.files = Some(ExecutionSupport {
            keys,
            artifacts,
            templates,
        });
        Ok(self)
    }

    pub async fn transfer_file(
        &self,
        caller: &PlaneCaller,
        input: TransferFileInput,
    ) -> Result<FileOperation> {
        let actor = ComputerActor::from_verified(&caller.identity)?;
        let support = self.files.as_ref().ok_or(ApplicationError::SetupRequired)?;
        let authority = self
            .store
            .file_transfer_authority(&actor, input.computer_id, input.grant_id)
            .await?;
        if !support
            .templates
            .contains(&authority.computer()?.template_fingerprint)
        {
            return Err(ApplicationError::Unavailable);
        }
        let payload = FileTransferPayload::new(input.transfer, input.limits)?;
        let mut operation = self
            .store
            .queue_file_transfer(&actor, authority, input.request_id, &payload, &support.keys)
            .await?;
        if !operation.task_projected() {
            self.store.ensure_file_task(&operation).await?;
        }
        if operation.stage() == FileTransferStage::Queued
            && let Some(request) = operation.file_capability_request(&support.keys)?
        {
            // Persist only a Task-bound receipt. An unavailable Artifact service
            // leaves the same admitted Task repairable by an exact caller retry.
            let preparation = async {
                let access = match request {
                    FileCapabilityRequest::Import(request) => FileTransferAccess::Import {
                        capability: support
                            .artifacts
                            .issue_read_capability(caller, &request)
                            .await
                            .ok()?,
                    },
                    FileCapabilityRequest::Export(request) => FileTransferAccess::Export {
                        capability: support
                            .artifacts
                            .issue_write_capability(caller, &request)
                            .await
                            .ok()?,
                    },
                };
                self.store
                    .attach_file_artifact_access(&operation, access, &support.keys)
                    .await
                    .ok()
            };
            match tokio::time::timeout(Duration::from_secs(5), preparation).await {
                Ok(Some(prepared)) => operation = prepared,
                _ => {
                    tracing::warn!(transfer_id = %operation.transfer_id(), "Computer file preparation awaits an authorized retry")
                }
            }
        }
        self.store
            .authorize_file_task(&actor, operation.transfer_id(), FileTaskAction::Observe)
            .await?;
        Ok(operation)
    }
}
