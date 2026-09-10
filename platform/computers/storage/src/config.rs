use crate::{Template, transport::TlsConfig};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, path::PathBuf};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageConfig {
    pub provider_id: uuid::Uuid,
    pub namespace: String,
    pub root: PathBuf,
    pub reserve_bytes: u64,
    pub templates: Vec<Template>,
    pub docker_socket: PathBuf,
    pub plugin_name: String,
    pub plugin_socket: PathBuf,
    pub listen: SocketAddr,
    pub tls: TlsConfig,
}
