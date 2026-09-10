//! The local engine is an observed trust boundary, never an ambient endpoint.
use crate::{Result, StorageError};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{collections::BTreeMap, path::Path, time::Duration};
use uuid::Uuid;
use veoveo_computers_runtime::RegisteredConsumer;

const API: &str = "http://localhost/v1.53";
const MAX_RESPONSE: usize = 1024 * 1024;

#[derive(Clone)]
pub struct Docker {
    client: Client,
    engine_id: Uuid,
    driver: String,
}
#[derive(Deserialize)]
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
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Create<'a> {
    name: &'a str,
    driver: &'a str,
}
impl Docker {
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
        let client = Client::builder()
            .unix_socket(socket.to_owned())
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| StorageError::BackendUnavailable)?;
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
    /// Called without the filesystem mutex: Create calls this plugin back.
    /// A lost mutation reply returns uncertainty. A later call first reads the
    /// exact name, and cannot adopt a volume using another driver or options.
    pub async fn ensure_volume(&self, name: &str) -> Result<()> {
        crate::service::volume_id(name)?;
        self.verify_engine().await?;
        let response = self
            .client
            .get(format!("{API}/volumes/{name}"))
            .send()
            .await
            .map_err(|_| StorageError::BackendUnavailable)?;
        let volume: Volume = if response.status() == StatusCode::NOT_FOUND {
            let created = self
                .client
                .post(format!("{API}/volumes/create"))
                .json(&Create {
                    name,
                    driver: &self.driver,
                })
                .send()
                .await
                .map_err(|_| StorageError::BackendUnavailable)?;
            decode(created).await?
        } else {
            decode(response).await?
        };
        if volume.name != name
            || volume.driver != self.driver
            || volume.options.is_some_and(|options| !options.is_empty())
        {
            return Err(StorageError::IdentityMismatch);
        }
        self.verify_engine().await?;
        Ok(())
    }
}

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
