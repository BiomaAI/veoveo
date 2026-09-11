use super::{CommandOperation, CommandStage, model};
use crate::{
    ComputerError, ComputersStore, Result,
    secrets::{CommandOutputAccess, ComputerKeyRing},
};
use chrono::{TimeDelta, Utc};
use surrealdb::types::SurrealValue;
use veoveo_mcp_contract::{IssueArtifactWriteCapabilityRequest, IssuedArtifactWriteCapability};
use veoveo_platform_store::OpenObject;

/// Publication has a separate finite allowance after foreground execution. This
/// is a capability lifetime bound, not permission to continue after revocation.
pub(super) const OUTPUT_PUBLICATION_SECONDS: u32 = 120;
pub(super) const PREPARATION_ALLOWANCE_SECONDS: u32 = 300;

fn object(value: &impl serde::Serialize) -> Result<OpenObject> {
    serde_json::from_value(serde_json::to_value(value).map_err(|_| ComputerError::Unavailable)?)
        .map_err(|_| ComputerError::Unavailable)
}

impl CommandOperation {
    /// Trusted service preparation. The service forwards the real caller identity
    /// to Artifacts; it cannot choose a smaller inherited-label floor.
    pub fn output_capability_request(
        &self,
        keys: &ComputerKeyRing,
    ) -> Result<Option<IssueArtifactWriteCapabilityRequest>> {
        if self.stage != CommandStage::Queued {
            return Err(ComputerError::InvalidState);
        }
        let payload = keys.open(&self.binding, &self.sealed)?;
        if let Some(sealed) = &self.output_access {
            let access = keys.open_output_access(&self.binding, sealed)?;
            if access.capability().expires_at
                > Utc::now()
                    + TimeDelta::seconds(i64::from(
                        payload.limits().maximum_seconds + OUTPUT_PUBLICATION_SECONDS,
                    ))
            {
                return Ok(None);
            }
        }
        Ok(Some(IssueArtifactWriteCapabilityRequest {
            task_id: self.execution_id().to_string(),
            expires_at: Utc::now()
                + TimeDelta::seconds(i64::from(
                    payload.limits().maximum_seconds
                        + OUTPUT_PUBLICATION_SECONDS
                        + PREPARATION_ALLOWANCE_SECONDS,
                )),
            max_artifact_count: std::num::NonZeroU32::new(2).expect("two output streams"),
            max_total_bytes: std::num::NonZeroU64::new(u64::from(
                payload.limits().maximum_output_bytes,
            ))
            .ok_or(ComputerError::Unavailable)?,
            required_data_labels: self.binding.required_output_labels.clone(),
        }))
    }
}

impl ComputersStore {
    /// Attach a receipt obtained from the Artifact service while the real caller
    /// is present. The first capability with enough remaining lifetime wins. A queued
    /// receipt may be renewed once it cannot cover execution and publication. No provider effect is authorized by this method.
    pub async fn attach_command_output(
        &self,
        command: &CommandOperation,
        capability: IssuedArtifactWriteCapability,
        keys: &ComputerKeyRing,
    ) -> Result<CommandOperation> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.attach_output(command, capability, keys),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    async fn attach_output(
        &self,
        command: &CommandOperation,
        capability: IssuedArtifactWriteCapability,
        keys: &ComputerKeyRing,
    ) -> Result<CommandOperation> {
        let saved = self.read_output_command(command).await?;
        let payload = keys.open(&saved.binding, &saved.sealed)?;
        let access = CommandOutputAccess::new(capability, payload.limits().maximum_output_bytes)?;
        // Validate even a losing contender; wrong Task receipts are never accepted.
        let sealed = keys.seal_output_access(&saved.binding, &access)?;
        let expires = access.capability().expires_at;
        let required_seconds = payload.limits().maximum_seconds + OUTPUT_PUBLICATION_SECONDS;
        let required_until = Utc::now() + TimeDelta::seconds(i64::from(required_seconds));
        if expires <= required_until || expires > Utc::now() + TimeDelta::hours(24) {
            return Err(ComputerError::InvalidInput);
        }
        let previous = saved
            .output_access
            .as_ref()
            .map(|v| keys.open_output_access(&saved.binding, v))
            .transpose()?;
        if previous
            .as_ref()
            .is_some_and(|v| v.capability().expires_at > required_until)
        {
            return Ok(saved);
        }
        if saved.stage != CommandStage::Queued {
            return Err(ComputerError::InvalidState);
        }
        self.query(
            include_str!("../../queries/attach_command_output.surql"),
            vec![
                (
                    "execution",
                    super::record(saved.execution_id()).into_value(),
                ),
                ("provider", self.provider_instance_id.into_value()),
                ("binding", object(&saved.binding)?.into_value()),
                ("authority", object(&saved.authority)?.into_value()),
                (
                    "expected_output",
                    saved
                        .output_access
                        .as_ref()
                        .map(object)
                        .transpose()?
                        .into_value(),
                ),
                (
                    "previous_expires",
                    previous
                        .as_ref()
                        .map(|v| v.capability().expires_at)
                        .into_value(),
                ),
                ("expires", expires.into_value()),
                (
                    "required_budget",
                    surrealdb::types::Duration::from_secs(u64::from(required_seconds)).into_value(),
                ),
                ("sealed", object(&sealed)?.into_value()),
            ],
        )
        .await?;
        let selected = self.read_output_command(command).await?;
        let selected_access = keys.open_output_access(
            &selected.binding,
            selected
                .output_access
                .as_ref()
                .ok_or(ComputerError::Unavailable)?,
        )?;
        if selected_access.capability().expires_at
            <= Utc::now() + TimeDelta::seconds(i64::from(required_seconds))
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(selected)
    }
    async fn read_output_command(&self, command: &CommandOperation) -> Result<CommandOperation> {
        if command.binding.provider_instance_id != self.provider_instance_id {
            return Err(ComputerError::NotFound);
        }
        let mut response = self
            .query(
                "SELECT * FROM ONLY $execution;",
                vec![(
                    "execution",
                    super::record(command.execution_id()).into_value(),
                )],
            )
            .await?;
        let row: Option<model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let saved = CommandOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        if crate::identity::digest(&(&saved.binding, &saved.authority))?
            != crate::identity::digest(&(&command.binding, &command.authority))?
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(saved)
    }
}
