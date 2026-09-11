//! Private retained-file intent; no file body is persisted in the control database.
use super::{
    ComputerKeyRing,
    cipher::{SealedCommand, SecretKind},
};
use crate::{
    ComputerError, Result,
    api::{FileTransfer, FileTransferDirection, FileTransferLimits, MAX_TRANSFER_BYTES},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;
use veoveo_mcp_contract::DataLabelId;
use zeroize::Zeroizing;

pub(super) const MAX_FILE_PAYLOAD_BYTES: usize = 16 * 1024;

/// Immutable domain identities, authenticated separately from private paths.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileTransferBinding {
    pub transfer_id: Uuid,
    pub request_id: Uuid,
    pub computer_id: Uuid,
    pub instance_id: Uuid,
    pub provider_instance_id: Uuid,
    pub grant_id: Option<Uuid>,
    pub direction: FileTransferDirection,
    pub owner_key: String,
    pub actor_key: String,
    pub template_fingerprint: String,
    pub resource_id: String,
    pub process_id: String,
    pub required_labels: BTreeSet<DataLabelId>,
}
impl FileTransferBinding {
    pub(crate) fn validate(&self) -> Result<()> {
        self.aad().map(|_| ())
    }
    pub(super) fn aad(&self) -> Result<Vec<u8>> {
        let hash = |v: &str| {
            v.len() == 64
                && v.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        };
        let native =
            |v: &str| !v.is_empty() && v.len() <= 256 && v.bytes().all(|c| c.is_ascii_graphic());
        if [
            self.transfer_id,
            self.request_id,
            self.computer_id,
            self.instance_id,
            self.provider_instance_id,
        ]
        .iter()
        .any(Uuid::is_nil)
            || self.transfer_id.get_version_num() != 7
            || self.grant_id.is_some_and(|id| id.is_nil())
            || [&self.owner_key, &self.actor_key, &self.template_fingerprint]
                .iter()
                .any(|value| !hash(value))
            || !native(&self.resource_id)
            || !native(&self.process_id)
            || self.required_labels.len() > 256
        {
            return Err(ComputerError::InvalidInput);
        }
        serde_json::to_vec(&("veoveo.computer.file-transfer-envelope.v1", self))
            .map_err(|_| ComputerError::Unavailable)
    }
}

/// No Debug or Display: the request contains private retained filenames.
///
/// ```compile_fail
/// use veoveo_computers::secrets::FileTransferPayload;
/// fn cannot_log(value: FileTransferPayload) { let _ = format!("{value:?}"); }
/// ```
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileTransferPayload {
    transfer: FileTransfer,
    limits: FileTransferLimits,
}
impl FileTransferPayload {
    pub fn new(transfer: FileTransfer, limits: FileTransferLimits) -> Result<Self> {
        let payload = Self { transfer, limits };
        payload.validate()?;
        Ok(payload)
    }
    pub fn transfer(&self) -> &FileTransfer {
        &self.transfer
    }
    pub fn limits(&self) -> FileTransferLimits {
        self.limits
    }
    fn validate(&self) -> Result<()> {
        if !(1..=300).contains(&self.limits.maximum_seconds)
            || !(1..=MAX_TRANSFER_BYTES).contains(&self.limits.maximum_bytes)
        {
            return Err(ComputerError::InvalidInput);
        }
        match &self.transfer {
            FileTransfer::Import { artifact_id, .. } if artifact_id.get_version_num() != 7 => {
                return Err(ComputerError::InvalidInput);
            }
            FileTransfer::Export {
                filename,
                media_type,
                ..
            } => {
                let token = |v: &str| {
                    !v.is_empty()
                        && v.bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c))
                };
                if filename.is_empty()
                    || filename.len() > 255
                    || filename == "."
                    || filename == ".."
                    || filename.contains(['/', '\\'])
                    || filename.chars().any(char::is_control)
                    || media_type.len() > 255
                    || !media_type
                        .split_once('/')
                        .is_some_and(|(a, b)| token(a) && token(b))
                {
                    return Err(ComputerError::InvalidInput);
                }
            }
            _ => (),
        }
        Ok(())
    }
    fn encode(&self, binding: &FileTransferBinding) -> Result<Zeroizing<Vec<u8>>> {
        self.validate()?;
        if self.transfer.direction() != binding.direction {
            return Err(ComputerError::InvalidInput);
        }
        Ok(Zeroizing::new(
            serde_json::to_vec(self).map_err(|_| ComputerError::Unavailable)?,
        ))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct SealedFileTransfer(SealedCommand);

impl ComputerKeyRing {
    pub fn seal_file_transfer(
        &self,
        binding: &FileTransferBinding,
        payload: &FileTransferPayload,
    ) -> Result<SealedFileTransfer> {
        Ok(SealedFileTransfer(self.seal_bound_bytes(
            &binding.aad()?,
            &payload.encode(binding)?,
            SecretKind::FileTransfer,
        )?))
    }
    pub fn open_file_transfer(
        &self,
        binding: &FileTransferBinding,
        sealed: &SealedFileTransfer,
    ) -> Result<FileTransferPayload> {
        let bytes = self.open_bound_bytes(&binding.aad()?, &sealed.0, SecretKind::FileTransfer)?;
        let payload: FileTransferPayload =
            serde_json::from_slice(&bytes).map_err(|_| ComputerError::Unavailable)?;
        payload
            .encode(binding)
            .map_err(|_| ComputerError::Unavailable)?;
        Ok(payload)
    }
    pub fn matches_file_transfer(
        &self,
        binding: &FileTransferBinding,
        sealed: &SealedFileTransfer,
        payload: &FileTransferPayload,
    ) -> Result<bool> {
        self.open_file_transfer(binding, sealed)?;
        self.matches_payload_fingerprint(
            &binding.aad()?,
            &sealed.0,
            &payload.encode(binding)?,
            SecretKind::FileTransfer,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        api::{AutomationInterruption, RetainedFilePath},
        secrets::ComputerSealingKey,
    };

    fn ring(active: u128, ids: &[u128]) -> ComputerKeyRing {
        ComputerKeyRing::new(
            Uuid::from_u128(active),
            ids.iter()
                .map(|id| {
                    ComputerSealingKey::new(Uuid::from_u128(*id), Zeroizing::new([*id as u8; 32]))
                        .unwrap()
                })
                .collect(),
        )
        .unwrap()
    }
    fn binding() -> FileTransferBinding {
        FileTransferBinding {
            transfer_id: Uuid::now_v7(),
            request_id: Uuid::now_v7(),
            computer_id: Uuid::now_v7(),
            instance_id: Uuid::now_v7(),
            provider_instance_id: Uuid::now_v7(),
            grant_id: None,
            direction: FileTransferDirection::Export,
            owner_key: "a".repeat(64),
            actor_key: "b".repeat(64),
            template_fingerprint: "c".repeat(64),
            resource_id: "resource".into(),
            process_id: "process".into(),
            required_labels: BTreeSet::new(),
        }
    }
    fn payload(path: &str) -> FileTransferPayload {
        FileTransferPayload::new(
            FileTransfer::Export {
                path: RetainedFilePath::try_from(path.to_owned()).unwrap(),
                filename: "output.bin".into(),
                media_type: "application/octet-stream".into(),
            },
            FileTransferLimits {
                maximum_seconds: 30,
                maximum_bytes: 1024,
                on_interruption: AutomationInterruption::StopComputer,
            },
        )
        .unwrap()
    }
    #[test]
    fn file_intent_is_private_identity_bound_and_rotation_preserves_retries() {
        let original = ring(1, &[1]);
        let binding = binding();
        let payload = payload("private-transfer-fixture");
        let sealed = original.seal_file_transfer(&binding, &payload).unwrap();
        assert!(
            !serde_json::to_string(&sealed)
                .unwrap()
                .contains("private-transfer-fixture")
        );
        let rotated = ring(2, &[1, 2]);
        assert_eq!(
            rotated
                .open_file_transfer(&binding, &sealed)
                .unwrap()
                .transfer()
                .path()
                .as_str(),
            "private-transfer-fixture"
        );
        assert!(
            rotated
                .matches_file_transfer(&binding, &sealed, &payload)
                .unwrap()
        );
        assert!(ring(2, &[2]).open_file_transfer(&binding, &sealed).is_err());
        for field in [
            "transfer_id",
            "computer_id",
            "instance_id",
            "provider_instance_id",
            "grant_id",
            "process_id",
            "actor_key",
            "required_labels",
        ] {
            let mut changed = serde_json::to_value(&binding).unwrap();
            changed[field] = match field {
                "process_id" => "another-process".into(),
                "actor_key" => "d".repeat(64).into(),
                "required_labels" => serde_json::json!(["restricted"]),
                _ => Uuid::now_v7().to_string().into(),
            };
            let changed: FileTransferBinding = serde_json::from_value(changed).unwrap();
            assert!(
                rotated.open_file_transfer(&changed, &sealed).is_err(),
                "{field}"
            );
        }
    }
    #[test]
    fn changed_payload_and_invalid_limits_cannot_match_an_accepted_transfer() {
        let keys = ring(1, &[1]);
        let binding = binding();
        let original = payload("first");
        let sealed = keys.seal_file_transfer(&binding, &original).unwrap();
        assert!(
            !keys
                .matches_file_transfer(&binding, &sealed, &payload("second"))
                .unwrap()
        );
        let mut changed = payload("first");
        changed.limits.maximum_bytes = 512;
        assert!(
            !keys
                .matches_file_transfer(&binding, &sealed, &changed)
                .unwrap()
        );
        changed.limits.maximum_seconds = 301;
        assert!(keys.seal_file_transfer(&binding, &changed).is_err());
        let mut wrong_direction = binding.clone();
        wrong_direction.direction = FileTransferDirection::Import;
        assert!(
            keys.seal_file_transfer(&wrong_direction, &original)
                .is_err()
        );
        let mut tampered = serde_json::to_value(&sealed).unwrap();
        tampered["fingerprint"] = "A".repeat(44).into();
        assert!(
            keys.open_file_transfer(&binding, &serde_json::from_value(tampered).unwrap())
                .is_err()
        );
    }

    #[test]
    fn file_metadata_and_artifact_identity_are_validated_before_sealing() {
        for filename in ["", ".", "..", "../secret", "a/b", "a\\b", "a\nb"] {
            let mut request = payload("file");
            if let FileTransfer::Export {
                filename: value, ..
            } = &mut request.transfer
            {
                *value = filename.into();
            }
            assert!(request.validate().is_err());
        }
        for media_type in [
            "",
            "text",
            "text/",
            "/plain",
            "text/plain/extra",
            "text/plain\r\nx-header: bad",
        ] {
            let mut request = payload("file");
            if let FileTransfer::Export {
                media_type: value, ..
            } = &mut request.transfer
            {
                *value = media_type.into();
            }
            assert!(request.validate().is_err());
        }
        for id in [Uuid::nil(), Uuid::from_u128(1)] {
            assert!(
                FileTransferPayload::new(
                    FileTransfer::Import {
                        artifact_id: id,
                        path: RetainedFilePath::try_from("file".to_owned()).unwrap()
                    },
                    payload("file").limits
                )
                .is_err()
            );
        }
    }
}
