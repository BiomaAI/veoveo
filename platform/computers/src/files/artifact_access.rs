use super::FileOperation;
use crate::{
    ComputerError, ComputersStore, Result,
    api::FileTransferDirection,
    secrets::{ComputerKeyRing, FileTransferAccess},
};
use chrono::{TimeDelta, Utc};
use std::num::{NonZeroU32, NonZeroU64};
use surrealdb::types::SurrealValue;
use veoveo_mcp_contract::{
    ArtifactTaskId, IssueArtifactReadCapabilityRequest, IssueArtifactWriteCapabilityRequest,
};

pub const FILE_PREPARATION_SECONDS: u32 = 300;
pub const FILE_PUBLICATION_SECONDS: u32 = 120;

pub enum FileCapabilityRequest {
    Import(IssueArtifactReadCapabilityRequest),
    Export(IssueArtifactWriteCapabilityRequest),
}
impl FileOperation {
    pub fn file_capability_request(
        &self,
        keys: &ComputerKeyRing,
    ) -> Result<Option<FileCapabilityRequest>> {
        let payload = keys.open_file_transfer(&self.binding, &self.sealed)?;
        let required = Utc::now()
            + TimeDelta::seconds(i64::from(
                payload.limits().maximum_seconds + FILE_PUBLICATION_SECONDS,
            ));
        if let Some(sealed) = &self.access {
            let access = keys.open_file_access(&self.binding, sealed)?;
            if access.expires_at() > required {
                return Ok(None);
            }
        }
        let expires_at = required + TimeDelta::seconds(i64::from(FILE_PREPARATION_SECONDS));
        let max_total_bytes =
            NonZeroU64::new(payload.limits().maximum_bytes).ok_or(ComputerError::Unavailable)?;
        let max_artifact_count = NonZeroU32::new(1).expect("one file");
        Ok(Some(match self.binding.direction {
            FileTransferDirection::Import => {
                FileCapabilityRequest::Import(IssueArtifactReadCapabilityRequest {
                    task_id: ArtifactTaskId::parse(self.transfer_id().to_string())
                        .map_err(|_| ComputerError::Unavailable)?,
                    expires_at,
                    max_artifact_count,
                    max_total_bytes,
                })
            }
            FileTransferDirection::Export => {
                FileCapabilityRequest::Export(IssueArtifactWriteCapabilityRequest {
                    task_id: self.transfer_id().to_string(),
                    expires_at,
                    max_artifact_count,
                    max_total_bytes,
                    required_data_labels: self.binding.required_labels.clone(),
                })
            }
        }))
    }
}
impl ComputersStore {
    /// Store only a real Artifact-service receipt obtained while its caller is present.
    pub async fn attach_file_artifact_access(
        &self,
        operation: &FileOperation,
        access: FileTransferAccess,
        keys: &ComputerKeyRing,
    ) -> Result<FileOperation> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.attach_file_access(operation, access, keys),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    async fn attach_file_access(
        &self,
        operation: &FileOperation,
        access: FileTransferAccess,
        keys: &ComputerKeyRing,
    ) -> Result<FileOperation> {
        let saved = self.saved_file(operation).await?;
        let payload = keys.open_file_transfer(&saved.binding, &saved.sealed)?;
        let sealed = keys.seal_file_access(&saved.binding, &access)?;
        let required_seconds = payload.limits().maximum_seconds + FILE_PUBLICATION_SECONDS;
        let required = Utc::now() + TimeDelta::seconds(i64::from(required_seconds));
        if access.expires_at() <= required
            || access.expires_at() > Utc::now() + TimeDelta::hours(24)
        {
            return Err(ComputerError::InvalidInput);
        }
        let previous = saved
            .access
            .as_ref()
            .map(|value| keys.open_file_access(&saved.binding, value))
            .transpose()?;
        if previous
            .as_ref()
            .is_some_and(|value| value.expires_at() > required)
        {
            return Ok(saved);
        }
        self.query(
            include_str!("../../queries/attach_file_access.surql"),
            vec![
                ("transfer", super::record(saved.transfer_id()).into_value()),
                ("provider", self.provider_instance_id.into_value()),
                ("binding", super::object(&saved.binding)?.into_value()),
                ("authority", super::object(&saved.authority)?.into_value()),
                (
                    "expected_access",
                    saved
                        .access
                        .as_ref()
                        .map(super::object)
                        .transpose()?
                        .into_value(),
                ),
                (
                    "previous_expires",
                    previous
                        .as_ref()
                        .map(FileTransferAccess::expires_at)
                        .into_value(),
                ),
                ("expires", access.expires_at().into_value()),
                (
                    "required_budget",
                    surrealdb::types::Duration::from_secs(u64::from(required_seconds)).into_value(),
                ),
                ("sealed", super::object(&sealed)?.into_value()),
            ],
        )
        .await?;
        let selected = self.saved_file(operation).await?;
        let access = keys.open_file_access(
            &selected.binding,
            selected.access.as_ref().ok_or(ComputerError::Unavailable)?,
        )?;
        if access.expires_at() <= Utc::now() + TimeDelta::seconds(i64::from(required_seconds)) {
            return Err(ComputerError::Unavailable);
        }
        Ok(selected)
    }
}
