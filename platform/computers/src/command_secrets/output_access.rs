use super::{
    CommandBinding, CommandKeyRing,
    cipher::{SealedCommand, SecretKind},
};
use crate::{ComputerError, Result};
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::IssuedArtifactWriteCapability;
use zeroize::Zeroizing;

pub(super) const MAX_OUTPUT_ACCESS_BYTES: usize = 8192;

/// Internal receipt from the Artifact service. Neither the command Task nor audit
/// projection may serialize this recoverable write secret.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandOutputAccess {
    capability: IssuedArtifactWriteCapability,
    maximum_output_bytes: u32,
}
impl CommandOutputAccess {
    pub fn new(
        capability: IssuedArtifactWriteCapability,
        maximum_output_bytes: u32,
    ) -> Result<Self> {
        let access = Self {
            capability,
            maximum_output_bytes,
        };
        access.validate()?;
        Ok(access)
    }
    fn validate(&self) -> Result<()> {
        if uuid::Uuid::parse_str(&self.capability.task_id)
            .ok()
            .is_none_or(|id| id.get_version_num() != 7)
            || !(1..=67108864).contains(&self.maximum_output_bytes)
            || self.capability.secret.expose_secret().len() > 256
        {
            return Err(ComputerError::InvalidInput);
        }
        Ok(())
    }
    pub fn capability(&self) -> &IssuedArtifactWriteCapability {
        &self.capability
    }
    pub fn maximum_output_bytes(&self) -> u32 {
        self.maximum_output_bytes
    }
    pub(super) fn check_binding(&self, binding: &CommandBinding) -> Result<()> {
        self.validate()?;
        if self.capability.task_id != binding.execution_id.to_string() {
            return Err(ComputerError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct SealedOutputAccess(SealedCommand);

impl CommandKeyRing {
    pub fn seal_output_access(
        &self,
        binding: &CommandBinding,
        access: &CommandOutputAccess,
    ) -> Result<SealedOutputAccess> {
        access.check_binding(binding)?;
        let bytes =
            Zeroizing::new(serde_json::to_vec(access).map_err(|_| ComputerError::Unavailable)?);
        Ok(SealedOutputAccess(self.seal_bytes(
            binding,
            &bytes,
            SecretKind::OutputAccess,
        )?))
    }
    pub fn open_output_access(
        &self,
        binding: &CommandBinding,
        sealed: &SealedOutputAccess,
    ) -> Result<CommandOutputAccess> {
        let bytes = self.open_bytes(binding, &sealed.0, SecretKind::OutputAccess)?;
        let access: CommandOutputAccess =
            serde_json::from_slice(&bytes).map_err(|_| ComputerError::Unavailable)?;
        access
            .check_binding(binding)
            .map_err(|_| ComputerError::Unavailable)?;
        Ok(access)
    }
}
