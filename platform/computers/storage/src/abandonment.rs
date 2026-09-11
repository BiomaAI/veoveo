//! Permanently retire a never-claimed admission without discarding its home.
use crate::{HomeIdentity, Result, Service, StorageError, WriterState};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Abandonment {
    pub operation_id: Uuid,
    pub source: HomeIdentity,
    pub target: HomeIdentity,
}
impl Abandonment {
    pub(crate) fn validate(&self) -> Result<()> {
        self.source.binding()?;
        self.target.binding()?;
        if self.operation_id.is_nil()
            || self.source.provider_id != self.target.provider_id
            || self.source.computer_id != self.target.computer_id
            || self.source.instance_id == self.target.instance_id
            || self.target.instance_id == self.target.computer_id
        {
            return Err(StorageError::InvalidIdentity);
        }
        Ok(())
    }
}
impl Service {
    pub async fn abandon(&self, request: Abandonment) -> Result<u64> {
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
                filesystem.journal().verify_abandonment(&request)?;
            } else {
                if record.identity() != &request.source
                    || record.writer() != &WriterState::Unclaimed
                {
                    return Err(StorageError::IdentityMismatch);
                }
                filesystem.journal().check_abandonment_target(&request)?;
                let empty = self.docker.prove_unclaimed(&name).await?;
                let fenced = filesystem.fence_unclaimed(&request.source, empty).await?;
                filesystem.commit_abandonment(&request, fenced)?;
            }
            filesystem.restore(&request.target).await?;
        }
        self.docker.ensure_volume(&name).await?;
        Ok(capacity)
    }
}
