//! Shared admission for the worker protocol and daemon plugin. The filesystem
//! mutex serializes mount checks with physical allocation and writer transitions.
use crate::{AllocationState, Docker, Filesystem, HomeIdentity, Result, StorageError};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use tokio::sync::{Mutex, Semaphore};
use uuid::Uuid;
use veoveo_computers_runtime::{Binding, PersistentHome};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Template {
    pub fingerprint: String,
    pub capacity_bytes: u64,
}
pub struct Service {
    pub(crate) filesystem: Mutex<Filesystem>,
    pub(crate) docker: Docker,
    provider_id: Uuid,
    templates: BTreeMap<String, u64>,
    pub(crate) calls: Arc<Semaphore>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Volume {
    pub name: String,
    pub mountpoint: PathBuf,
}
impl Service {
    pub async fn new(
        filesystem: Filesystem,
        docker: Docker,
        templates: Vec<Template>,
    ) -> Result<Arc<Self>> {
        if templates.is_empty() || templates.len() > 64 {
            return Err(StorageError::InvalidIdentity);
        }
        let provider_id = filesystem.journal().identity().provider_id;
        let mut admitted = BTreeMap::new();
        for template in templates {
            Binding::new(Uuid::from_u128(1), template.fingerprint.clone())
                .map_err(|_| StorageError::InvalidIdentity)?;
            if !(512 * 1024 * 1024..=256 * 1024 * 1024 * 1024).contains(&template.capacity_bytes)
                || !template.capacity_bytes.is_multiple_of(4096)
                || admitted
                    .insert(template.fingerprint, template.capacity_bytes)
                    .is_some()
            {
                return Err(StorageError::InvalidIdentity);
            }
        }
        if docker.verify_engine().await? != filesystem.journal().identity().engine_id {
            return Err(StorageError::IdentityMismatch);
        }
        Ok(Arc::new(Self {
            filesystem: Mutex::new(filesystem),
            docker,
            provider_id,
            templates: admitted,
            calls: Arc::new(Semaphore::new(32)),
        }))
    }
    pub(crate) fn capacity(&self, provider: Uuid, fingerprint: &str) -> Result<u64> {
        if provider != self.provider_id {
            return Err(StorageError::IdentityMismatch);
        }
        self.templates
            .get(fingerprint)
            .copied()
            .ok_or(StorageError::IdentityMismatch)
    }
    pub async fn ready(&self, provider: Uuid, fingerprint: &str) -> Result<u64> {
        let capacity = self.capacity(provider, fingerprint)?;
        self.docker.verify_engine().await?;
        Ok(capacity)
    }
    pub async fn prepare(&self, home: HomeIdentity) -> Result<u64> {
        let capacity = self.capacity(home.provider_id, &home.template_fingerprint)?;
        self.docker.verify_engine().await?;
        {
            let mut filesystem = self.filesystem.lock().await;
            filesystem.prepare(home.clone(), capacity).await?;
        }
        self.docker
            .ensure_volume(&volume_name(home.computer_id)?)
            .await?;
        Ok(capacity)
    }
    pub async fn restore(&self, home: HomeIdentity) -> Result<u64> {
        let capacity = self.capacity(home.provider_id, &home.template_fingerprint)?;
        self.docker.verify_engine().await?;
        {
            let mut filesystem = self.filesystem.lock().await;
            let record = filesystem
                .journal()
                .load(home.computer_id)?
                .ok_or(StorageError::RecoveryRequired)?;
            if record.capacity_bytes() != capacity {
                return Err(StorageError::IdentityMismatch);
            }
            filesystem.restore(&home).await?;
        }
        self.docker
            .ensure_volume(&volume_name(home.computer_id)?)
            .await?;
        Ok(capacity)
    }
    pub(crate) async fn volume(&self, name: &str, mount: bool) -> Result<Volume> {
        let computer = volume_id(name)?;
        let mut filesystem = self.filesystem.lock().await;
        let record = filesystem
            .journal()
            .load(computer)?
            .ok_or(StorageError::IdentityMismatch)?;
        if !matches!(record.state(), AllocationState::Ready { .. }) {
            return Err(StorageError::RecoveryRequired);
        }
        let home = record.identity();
        if self.capacity(home.provider_id, &home.template_fingerprint)? != record.capacity_bytes() {
            return Err(StorageError::IdentityMismatch);
        }
        let mountpoint = if mount {
            let writer = filesystem.journal().identity().writer(home)?;
            let engine = self.docker.verify_engine().await?;
            let consumers = self.docker.consumers(name).await?;
            if !writer.matches(engine, &consumers) {
                return Err(StorageError::WriterDenied);
            }
            // Recheck the socket after observation, before exposing a mount.
            self.docker.verify_engine().await?;
            filesystem.restore(home).await?
        } else {
            filesystem.journal().directory(computer)?.join("mount")
        };
        Ok(Volume {
            name: name.into(),
            mountpoint,
        })
    }
    pub(crate) async fn volumes(&self) -> Result<Vec<Volume>> {
        let filesystem = self.filesystem.lock().await;
        filesystem
            .journal()
            .list()?
            .into_iter()
            .filter(|record| matches!(record.state(), AllocationState::Ready { .. }))
            .map(|record| {
                Ok(Volume {
                    name: volume_name(record.identity().computer_id)?,
                    mountpoint: filesystem
                        .journal()
                        .directory(record.identity().computer_id)?
                        .join("mount"),
                })
            })
            .collect()
    }
}
pub(crate) fn volume_name(computer: Uuid) -> Result<String> {
    PersistentHome::volume_name(computer).map_err(|_| StorageError::InvalidIdentity)
}
pub(crate) fn volume_id(name: &str) -> Result<Uuid> {
    let id = name
        .strip_prefix("veoveo-computer-")
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or(StorageError::InvalidIdentity)?;
    if name != volume_name(id)? {
        return Err(StorageError::InvalidIdentity);
    }
    Ok(id)
}
