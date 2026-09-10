//! An admitted instance changes only after its recorded physical writer is fenced.
use crate::{HomeIdentity, Result, Service, StorageError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use veoveo_computers_runtime::RegisteredConsumer;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhysicalWriter {
    container_id: String,
    resource_id: String,
}
impl PhysicalWriter {
    pub(crate) fn observed(consumer: &RegisteredConsumer) -> Result<Self> {
        let writer = Self {
            container_id: consumer.container_id().into(),
            resource_id: consumer
                .resource_id()
                .ok_or(StorageError::WriterDenied)?
                .into(),
        };
        writer.validate()?;
        Ok(writer)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if self.container_id.len() != 64
            || !self
                .container_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !valid_resource(&self.resource_id)
        {
            return Err(StorageError::RecoveryRequired);
        }
        Ok(())
    }
    pub fn container_id(&self) -> &str {
        &self.container_id
    }
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum WriterState {
    Unclaimed,
    Claimed { writer: PhysicalWriter },
}
impl WriterState {
    pub(crate) fn validate(&self) -> Result<()> {
        match self {
            Self::Unclaimed => Ok(()),
            Self::Claimed { writer } => writer.validate(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Handoff {
    pub operation_id: Uuid,
    pub source: HomeIdentity,
    pub target: HomeIdentity,
    pub source_resource_id: String,
}
impl Handoff {
    pub(crate) fn validate(&self) -> Result<()> {
        self.source.binding()?;
        self.target.binding()?;
        if self.operation_id.is_nil()
            || self.source.provider_id != self.target.provider_id
            || self.source.computer_id != self.target.computer_id
            || self.source.instance_id == self.target.instance_id
            || self.target.instance_id == self.target.computer_id
            || !valid_resource(&self.source_resource_id)
        {
            return Err(StorageError::InvalidIdentity);
        }
        Ok(())
    }
}
fn valid_resource(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
impl Service {
    pub async fn handoff(&self, request: Handoff) -> Result<u64> {
        request.validate()?;
        let capacity = self.capacity(
            request.target.provider_id,
            &request.target.template_fingerprint,
        )?;
        self.capacity(
            request.source.provider_id,
            &request.source.template_fingerprint,
        )?;
        let name = crate::service::volume_name(request.target.computer_id)?;
        {
            let mut filesystem = self.filesystem.lock().await;
            let record = filesystem
                .journal()
                .load(request.source.computer_id)?
                .ok_or(StorageError::RecoveryRequired)?;
            if record.capacity_bytes() != capacity {
                return Err(StorageError::IdentityMismatch);
            }
            if record.identity() == &request.target {
                // A lost reply can be resolved even after the target mounted.
                // An old transition cannot roll back a later admitted instance.
                filesystem.journal().verify_handoff(&request)?;
            } else {
                if record.identity() != &request.source {
                    return Err(StorageError::IdentityMismatch);
                }
                let WriterState::Claimed { writer } = record.writer() else {
                    return Err(StorageError::RecoveryRequired);
                };
                if writer.resource_id() != request.source_resource_id {
                    return Err(StorageError::IdentityMismatch);
                }
                filesystem
                    .journal()
                    .check_handoff_target(&request, writer)?;
                let removed = self.docker.prove_removed(writer, &name).await?;
                let fenced = filesystem.fence(&request.source, removed).await?;
                filesystem.commit_handoff(&request, fenced)?;
            }
            filesystem.restore(&request.target).await?;
        }
        self.docker.ensure_volume(&name).await?;
        Ok(capacity)
    }
}
