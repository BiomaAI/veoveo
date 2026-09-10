use crate::{Result, StorageError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use veoveo_computers_runtime::{Binding, RetainedWriter};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostIdentity {
    pub provider_id: Uuid,
    pub engine_id: Uuid,
    pub namespace: String,
}
impl HostIdentity {
    pub fn validate(&self) -> Result<()> {
        RetainedWriter::new(
            self.provider_id,
            self.engine_id,
            self.namespace.clone(),
            Binding::new(Uuid::from_u128(1), "0".repeat(64))
                .map_err(|_| StorageError::InvalidIdentity)?,
        )
        .map(|_| ())
        .map_err(|_| StorageError::InvalidIdentity)
    }
    pub fn writer(&self, home: &HomeIdentity) -> Result<RetainedWriter> {
        if home.provider_id != self.provider_id {
            return Err(StorageError::IdentityMismatch);
        }
        RetainedWriter::new(
            self.provider_id,
            self.engine_id,
            self.namespace.clone(),
            home.binding()?,
        )
        .map_err(|_| StorageError::InvalidIdentity)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HomeIdentity {
    pub provider_id: Uuid,
    pub computer_id: Uuid,
    pub instance_id: Uuid,
    pub template_fingerprint: String,
}
impl HomeIdentity {
    pub fn binding(&self) -> Result<Binding> {
        if self.provider_id.is_nil() || self.instance_id.is_nil() {
            return Err(StorageError::InvalidIdentity);
        }
        let result = if self.computer_id == self.instance_id {
            Binding::new(self.computer_id, self.template_fingerprint.clone())
        } else {
            Binding::replacement(
                self.computer_id,
                self.instance_id,
                self.template_fingerprint.clone(),
            )
        };
        result.map_err(|_| StorageError::InvalidIdentity)
    }
}
