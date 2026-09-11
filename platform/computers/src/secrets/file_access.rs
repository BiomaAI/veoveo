//! Protected Artifact capability for one accepted file-transfer Task.
use super::{
    ComputerKeyRing, FileTransferBinding,
    cipher::{SealedCommand, SecretKind},
};
use crate::{ComputerError, Result, api::FileTransferDirection};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{IssuedArtifactReadCapability, IssuedArtifactWriteCapability};
use zeroize::Zeroizing;

pub(super) const MAX_FILE_ACCESS_BYTES: usize = 8192;

/// Internal Artifact-service receipt. This is not a public input or gateway bearer.
///
/// ```compile_fail
/// use veoveo_computers::secrets::FileTransferAccess;
/// fn cannot_log(value: FileTransferAccess) { let _ = format!("{value:?}"); }
/// ```
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FileTransferAccess {
    Import {
        capability: IssuedArtifactReadCapability,
    },
    Export {
        capability: IssuedArtifactWriteCapability,
    },
}
impl FileTransferAccess {
    pub fn expires_at(&self) -> DateTime<Utc> {
        match self {
            Self::Import { capability } => capability.expires_at,
            Self::Export { capability } => capability.expires_at,
        }
    }
    fn check_binding(&self, binding: &FileTransferBinding) -> Result<()> {
        let valid = match self {
            Self::Import { capability } => {
                binding.direction == FileTransferDirection::Import
                    && capability.task_id.as_uuid() == binding.transfer_id
            }
            Self::Export { capability } => {
                binding.direction == FileTransferDirection::Export
                    && capability.task_id == binding.transfer_id.to_string()
            }
        };
        if !valid {
            return Err(ComputerError::InvalidInput);
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct SealedFileTransferAccess(SealedCommand);

impl ComputerKeyRing {
    pub fn seal_file_access(
        &self,
        binding: &FileTransferBinding,
        access: &FileTransferAccess,
    ) -> Result<SealedFileTransferAccess> {
        access.check_binding(binding)?;
        let bytes =
            Zeroizing::new(serde_json::to_vec(access).map_err(|_| ComputerError::Unavailable)?);
        Ok(SealedFileTransferAccess(self.seal_bound_bytes(
            &binding.aad()?,
            &bytes,
            SecretKind::FileAccess,
        )?))
    }
    pub fn open_file_access(
        &self,
        binding: &FileTransferBinding,
        sealed: &SealedFileTransferAccess,
    ) -> Result<FileTransferAccess> {
        let bytes = self.open_bound_bytes(&binding.aad()?, &sealed.0, SecretKind::FileAccess)?;
        let access: FileTransferAccess =
            serde_json::from_slice(&bytes).map_err(|_| ComputerError::Unavailable)?;
        access
            .check_binding(binding)
            .map_err(|_| ComputerError::Unavailable)?;
        Ok(access)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::ComputerSealingKey;
    use uuid::Uuid;
    use veoveo_mcp_contract::{
        ArtifactReadCapabilityId, ArtifactReadCapabilitySecret, ArtifactTaskId,
    };

    #[test]
    fn read_capability_is_task_bound_and_cannot_replace_a_request_envelope() {
        let id = Uuid::now_v7();
        let keys = ComputerKeyRing::new(
            id,
            vec![ComputerSealingKey::new(id, Zeroizing::new([7; 32])).unwrap()],
        )
        .unwrap();
        let binding = FileTransferBinding {
            transfer_id: Uuid::now_v7(),
            request_id: Uuid::now_v7(),
            computer_id: Uuid::now_v7(),
            instance_id: Uuid::now_v7(),
            provider_instance_id: Uuid::now_v7(),
            grant_id: None,
            direction: FileTransferDirection::Import,
            owner_key: "a".repeat(64),
            actor_key: "b".repeat(64),
            template_fingerprint: "c".repeat(64),
            resource_id: "resource".into(),
            process_id: "process".into(),
            required_labels: Default::default(),
        };
        let access = FileTransferAccess::Import {
            capability: IssuedArtifactReadCapability {
                capability_id: ArtifactReadCapabilityId::new(),
                secret: ArtifactReadCapabilitySecret::new(
                    "private-read-capability-fixture-1234567890",
                )
                .unwrap(),
                task_id: ArtifactTaskId::parse(binding.transfer_id.to_string()).unwrap(),
                expires_at: Utc::now() + chrono::TimeDelta::minutes(10),
            },
        };
        let sealed = keys.seal_file_access(&binding, &access).unwrap();
        assert!(
            !serde_json::to_string(&sealed)
                .unwrap()
                .contains("private-read-capability")
        );
        assert_eq!(
            keys.open_file_access(&binding, &sealed)
                .unwrap()
                .expires_at(),
            access.expires_at()
        );
        let mut changed = binding.clone();
        changed.transfer_id = Uuid::now_v7();
        assert!(keys.seal_file_access(&changed, &access).is_err());
        assert!(keys.open_file_access(&changed, &sealed).is_err());
        let request = serde_json::from_value(serde_json::to_value(&sealed).unwrap()).unwrap();
        assert!(keys.open_file_transfer(&binding, &request).is_err());
        changed = binding;
        changed.direction = FileTransferDirection::Export;
        assert!(keys.seal_file_access(&changed, &access).is_err());
        let write = FileTransferAccess::Export {
            capability: IssuedArtifactWriteCapability {
                capability_id: veoveo_mcp_contract::ArtifactWriteCapabilityId::new(),
                secret: veoveo_mcp_contract::ArtifactWriteCapabilitySecret::new(
                    "private-write-capability-fixture-1234567890",
                )
                .unwrap(),
                task_id: changed.transfer_id.to_string(),
                expires_at: access.expires_at(),
            },
        };
        let sealed = keys.seal_file_access(&changed, &write).unwrap();
        assert_eq!(
            keys.open_file_access(&changed, &sealed)
                .unwrap()
                .expires_at(),
            write.expires_at()
        );
        changed.transfer_id = Uuid::now_v7();
        assert!(keys.seal_file_access(&changed, &write).is_err());
        assert!(keys.open_file_access(&changed, &sealed).is_err());
    }
}
