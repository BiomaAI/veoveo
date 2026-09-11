//! Provider-independent protection of a maintenance job's opaque policy snapshot.
use super::{
    ComputerKeyRing,
    cipher::{SealedCommand, SecretKind},
};
use crate::{ComputerError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;
use veoveo_mcp_contract::DataLabelId;
use zeroize::Zeroizing;

pub(super) const MAX_CHECKPOINT_BYTES: usize = 1024 * 1024 + 4096;

/// Exact domain-owned identities authenticated with the sensitive checkpoint.
/// This binding neither authorizes admission nor proves source-writer removal.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceBinding {
    pub operation_id: Uuid,
    pub request_id: Uuid,
    pub computer_id: Uuid,
    pub provider_instance_id: Uuid,
    pub owner_key: String,
    pub actor_key: String,
    pub source_instance_id: Uuid,
    pub target_instance_id: Uuid,
    pub source_template_fingerprint: String,
    pub target_template_fingerprint: String,
    pub source_resource_id: String,
    pub source_process_id: String,
    pub required_labels: BTreeSet<DataLabelId>,
}
impl MaintenanceBinding {
    fn aad(&self) -> Result<Vec<u8>> {
        let hash = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        let native =
            |s: &str| !s.is_empty() && s.len() <= 256 && s.bytes().all(|b| b.is_ascii_graphic());
        if [
            self.operation_id,
            self.request_id,
            self.computer_id,
            self.provider_instance_id,
            self.source_instance_id,
            self.target_instance_id,
        ]
        .iter()
        .any(Uuid::is_nil)
            || self.operation_id.get_version_num() != 7
            || self.target_instance_id == self.source_instance_id
            || self.target_instance_id == self.computer_id
            || [
                &self.owner_key,
                &self.actor_key,
                &self.source_template_fingerprint,
                &self.target_template_fingerprint,
            ]
            .iter()
            .any(|value| !hash(value))
            || !native(&self.source_resource_id)
            || !native(&self.source_process_id)
            || self.required_labels.len() > 256
        {
            return Err(ComputerError::InvalidInput);
        }
        serde_json::to_vec(&("veoveo.computer.maintenance-envelope.v1", self))
            .map_err(|_| ComputerError::Unavailable)
    }
}

/// Sensitive provider-adapter encoding, validated by that adapter after opening.
/// It carries no user bearer credential and belongs only in encrypted persistence.
///
/// ```compile_fail
/// use veoveo_computers::secrets::MaintenanceCheckpoint;
/// fn cannot_log(value: MaintenanceCheckpoint) { let _ = format!("{value:?}"); }
/// ```
pub struct MaintenanceCheckpoint(Zeroizing<Vec<u8>>);
impl MaintenanceCheckpoint {
    pub fn new(bytes: Zeroizing<Vec<u8>>) -> Result<Self> {
        if !(12..=MAX_CHECKPOINT_BYTES).contains(&bytes.len()) {
            return Err(ComputerError::InvalidInput);
        }
        Ok(Self(bytes))
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Private persistence only. Its fingerprint is keyed and its purpose is distinct
/// from command and output-access envelopes even when keys are shared.
///
/// ```compile_fail
/// use veoveo_computers::secrets::SealedMaintenanceCheckpoint;
/// fn cannot_log(value: SealedMaintenanceCheckpoint) { let _ = format!("{value:?}"); }
/// ```
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct SealedMaintenanceCheckpoint(SealedCommand);

impl ComputerKeyRing {
    pub fn seal_maintenance(
        &self,
        binding: &MaintenanceBinding,
        checkpoint: &MaintenanceCheckpoint,
    ) -> Result<SealedMaintenanceCheckpoint> {
        Ok(SealedMaintenanceCheckpoint(self.seal_bound_bytes(
            &binding.aad()?,
            checkpoint.bytes(),
            SecretKind::Maintenance,
        )?))
    }
    pub fn open_maintenance(
        &self,
        binding: &MaintenanceBinding,
        sealed: &SealedMaintenanceCheckpoint,
    ) -> Result<MaintenanceCheckpoint> {
        MaintenanceCheckpoint::new(self.open_bound_bytes(
            &binding.aad()?,
            &sealed.0,
            SecretKind::Maintenance,
        )?)
        .map_err(|_| ComputerError::Unavailable)
    }
}

#[cfg(test)]
mod tests;
