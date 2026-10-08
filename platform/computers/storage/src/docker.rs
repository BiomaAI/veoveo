//! The local engine is an observed trust boundary, never an ambient endpoint.
use crate::PhysicalWriter;
use crate::{Result, StorageError};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, path::Path, time::Duration};
use uuid::Uuid;
use veoveo_computers_runtime::{RegisteredConsumer, RetainedVolumeAdmission};

/// This proof only establishes Docker removal. Filesystem detachment is separate.
pub(crate) struct RemovedWriter {
    writer: PhysicalWriter,
    engine_id: Uuid,
}
/// Zero registered consumers on the original engine, checked while the caller
/// holds the allocation mutex. This alone does not exclude filesystem writers.
pub(crate) struct UnclaimedVolume {
    engine_id: Uuid,
}
impl UnclaimedVolume {
    pub(crate) fn engine_id(&self) -> Uuid {
        self.engine_id
    }
}
impl RemovedWriter {
    pub(crate) fn writer(&self) -> &PhysicalWriter {
        &self.writer
    }
    pub(crate) fn engine_id(&self) -> Uuid {
        self.engine_id
    }
}

const API: &str = "http://localhost/v1.53";
const MAX_RESPONSE: usize = 1024 * 1024;

/// Observed before a fresh filesystem allocation becomes visible to the plugin.
/// This authorizes one Create request, not adoption of a raced existing volume.
pub(crate) struct AbsentVolume {
    computer_id: Uuid,
    engine_id: Uuid,
}

#[derive(Clone)]
pub struct Docker {
    client: Client,
    engine_id: Uuid,
    driver: String,
}
#[derive(Deserialize, Serialize)]
struct Engine {
    #[serde(rename = "ID")]
    id: Uuid,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Volume {
    name: String,
    driver: String,
    options: Option<BTreeMap<String, String>>,
    labels: Option<BTreeMap<String, String>>,
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Create<'a> {
    name: &'a str,
    driver: &'a str,
    labels: BTreeMap<String, String>,
}
impl Docker {
    pub(crate) async fn prove_unclaimed(&self, volume: &str) -> Result<UnclaimedVolume> {
        self.verify_engine().await?;
        if !self.consumers(volume).await?.is_empty() {
            return Err(StorageError::WriterDenied);
        }
        self.verify_engine().await?;
        Ok(UnclaimedVolume {
            engine_id: self.engine_id,
        })
    }
    /// Enroll the engine reached through an explicit installation-owned socket.
    /// Journal::open must bind this identity before the helper serves requests.
    /// Reopening an existing journal rejects a different observed engine.
    pub async fn discover(socket: &Path, driver: String) -> Result<Self> {
        let client = engine_client(socket)?;
        let reply = client
            .get(format!("{API}/info"))
            .send()
            .await
            .map_err(|_| StorageError::BackendUnavailable)?;
        let engine: Engine = decode(reply).await?;
        Self::new(socket, engine.id, driver)
    }
    pub(crate) async fn prove_removed(
        &self,
        writer: &PhysicalWriter,
        volume: &str,
    ) -> Result<RemovedWriter> {
        writer.validate()?;
        self.verify_engine().await?;
        let response = self
            .client
            .get(format!("{API}/containers/{}/json", writer.container_id()))
            .send()
            .await
            .map_err(|_| StorageError::BackendUnavailable)?;
        if response.status() != StatusCode::NOT_FOUND {
            return Err(StorageError::WriterDenied);
        }
        if !self.consumers(volume).await?.is_empty() {
            return Err(StorageError::WriterDenied);
        }
        self.verify_engine().await?;
        Ok(RemovedWriter {
            writer: writer.clone(),
            engine_id: self.engine_id,
        })
    }
    pub fn new(socket: &Path, engine_id: Uuid, driver: String) -> Result<Self> {
        if !socket.is_absolute()
            || engine_id.is_nil()
            || driver.is_empty()
            || driver.len() > 63
            || !driver
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || driver == "local"
        {
            return Err(StorageError::InvalidIdentity);
        }
        let client = engine_client(socket)?;
        Ok(Self {
            client,
            engine_id,
            driver,
        })
    }
    pub async fn verify_engine(&self) -> Result<Uuid> {
        let reply = self
            .client
            .get(format!("{API}/info"))
            .send()
            .await
            .map_err(|_| StorageError::BackendUnavailable)?;
        let engine: Engine = decode(reply).await?;
        if engine.id != self.engine_id {
            return Err(StorageError::IdentityMismatch);
        }
        Ok(engine.id)
    }
    pub(crate) fn engine_id(&self) -> Uuid {
        self.engine_id
    }
    pub async fn consumers(&self, volume: &str) -> Result<Vec<RegisteredConsumer>> {
        crate::service::volume_id(volume)?;
        let filters = serde_json::to_string(&BTreeMap::from([("volume", [volume])]))
            .map_err(|_| StorageError::InvalidIdentity)?;
        let response = self
            .client
            .get(format!("{API}/containers/json"))
            .query(&[("all", "1"), ("filters", &filters)])
            .send()
            .await
            .map_err(|_| StorageError::BackendUnavailable)?;
        decode(response).await
    }
    pub(crate) async fn admit_absent_volume(&self, computer_id: Uuid) -> Result<AbsentVolume> {
        let name = crate::service::volume_name(computer_id)?;
        self.verify_engine().await?;
        let response = self
            .client
            .get(format!("{API}/volumes/{name}"))
            .send()
            .await
            .map_err(|_| StorageError::BackendUnavailable)?;
        if response.status() != StatusCode::NOT_FOUND {
            response
                .error_for_status()
                .map_err(|_| StorageError::BackendUnavailable)?;
            return Err(StorageError::IdentityMismatch);
        }
        self.verify_engine().await?;
        Ok(AbsentVolume {
            computer_id,
            engine_id: self.engine_id,
        })
    }

    /// Filesystem Ready publication follows absence admission. A discovery GET
    /// here would cache the plugin's volume before Engine approval labels exist.
    /// Create remains outside the filesystem mutex for plugin callbacks.
    pub(crate) async fn create_admitted_volume(&self, absent: AbsentVolume) -> Result<()> {
        if absent.engine_id != self.engine_id {
            return Err(StorageError::IdentityMismatch);
        }
        let name = crate::service::volume_name(absent.computer_id)?;
        self.verify_engine().await?;
        let response = self
            .client
            .post(format!("{API}/volumes/create"))
            .json(&Create {
                name: &name,
                driver: &self.driver,
                labels: RetainedVolumeAdmission::DEFAULT.labels(),
            })
            .send()
            .await
            .map_err(|_| StorageError::BackendUnavailable)?;
        let volume: Volume = decode(response).await?;
        self.admit_volume(&name, volume)?;
        self.verify_engine().await?;
        Ok(())
    }

    /// Reuse never dispatches Create. A missing observation after publication or
    /// a lost Create reply preserves the allocation and requires recovery.
    pub async fn inspect_volume(&self, name: &str) -> Result<()> {
        crate::service::volume_id(name)?;
        self.verify_engine().await?;
        let response = self
            .client
            .get(format!("{API}/volumes/{name}"))
            .send()
            .await
            .map_err(|_| StorageError::BackendUnavailable)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(StorageError::RecoveryRequired);
        }
        let volume: Volume = decode(response).await?;
        self.admit_volume(name, volume)?;
        self.verify_engine().await?;
        Ok(())
    }
    fn admit_volume(&self, name: &str, volume: Volume) -> Result<()> {
        if volume.name != name
            || volume.driver != self.driver
            || volume.options.is_some_and(|options| !options.is_empty())
            || volume
                .labels
                .as_ref()
                .is_none_or(|labels| !RetainedVolumeAdmission::DEFAULT.matches(labels))
        {
            return Err(StorageError::IdentityMismatch);
        }
        Ok(())
    }
}

fn engine_client(socket: &Path) -> Result<Client> {
    if !socket.is_absolute() {
        return Err(StorageError::InvalidIdentity);
    }
    Client::builder()
        .unix_socket(socket.to_owned())
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| StorageError::BackendUnavailable)
}

#[cfg(test)]
#[path = "docker_tests.rs"]
mod tests;

async fn decode<T: DeserializeOwned>(response: Response) -> Result<T> {
    let mut response = response
        .error_for_status()
        .map_err(|_| StorageError::BackendUnavailable)?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE as u64)
    {
        return Err(StorageError::BackendUnavailable);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| StorageError::BackendUnavailable)?
    {
        if chunk.len() > MAX_RESPONSE - bytes.len() {
            return Err(StorageError::BackendUnavailable);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| StorageError::BackendUnavailable)
}
