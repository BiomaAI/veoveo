use anyhow::{Result, ensure};
use serde::Deserialize;
use std::{collections::BTreeSet, net::Ipv4Addr};
use veoveo_computer_storage::{HostIdentity, StorageConfig, Template, transport::TlsConfig};

pub const STATE: &str = "/var/lib/veoveo-computers/state";
pub const RUN: &str = "/run/veoveo-computers";
pub const SOCKET: &str = "/run/veoveo-computers/docker.sock";
pub const PLUGIN: &str = "/run/veoveo-computers/volume.sock";
pub const TRUST: &str = "/etc/veoveo/computers/host-trust";
pub const PROVIDER_PORT: u16 = 8805;
pub const STORAGE_PORT: u16 = 8806;

#[derive(Deserialize)]
pub enum Schema {
    #[serde(rename = "veoveo.io/computer-host/v1")]
    V1,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn valid() -> Config {
        let image = format!("registry.internal:5000/computer@sha256:{}", "a".repeat(64));
        serde_json::from_value(serde_json::json!({
            "schema": "veoveo.io/computer-host/v1", "providerId": uuid::Uuid::from_u128(100),
            "namespace": "private-computers", "defaultImage": image, "images": [image],
            "templates": [{"fingerprint": "b".repeat(64), "capacityBytes": 536870912}],
            "reserveBytes": 536870912, "registry": {"authority": "registry.internal:5000", "transport": "development_http"},
            "bridgeAddress": "172.30.0.1", "networkPool": "172.31.0.0"
        })).unwrap()
    }
    #[test]
    fn host_catalog_rejects_unpinned_foreign_and_duplicate_inputs() {
        let mut config = valid();
        config.validate().unwrap();
        config.images.push(config.images[0].clone());
        assert!(config.validate().is_err());
        let mut config = valid();
        config.images[0] = "registry.internal:5000/computer:latest".into();
        assert!(config.validate().is_err());
        let mut config = valid();
        config.registry.authority = "other.internal:5000".into();
        assert!(config.validate().is_err());
        let mut config = valid();
        config.templates.push(config.templates[0].clone());
        assert!(config.validate().is_err());
    }
    #[test]
    fn private_networks_and_generated_storage_configuration_keep_one_identity() {
        let mut config = valid();
        let raw = serde_json::to_vec(&config.storage_config()).unwrap();
        let storage: StorageConfig = serde_json::from_slice(&raw).unwrap();
        assert_eq!(storage.provider_id, config.provider_id);
        assert_eq!(storage.namespace, config.namespace);
        config.network_pool = "172.30.0.0".parse().unwrap();
        assert!(config.validate().is_err());
        config.network_pool = "8.8.0.0".parse().unwrap();
        assert!(config.validate().is_err());
        config = valid();
        config.namespace = "injected\nnamespace".into();
        assert!(config.validate().is_err());
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub schema: Schema,
    pub provider_id: uuid::Uuid,
    pub namespace: String,
    pub default_image: String,
    pub images: Vec<String>,
    pub templates: Vec<Template>,
    pub reserve_bytes: u64,
    pub registry: Registry,
    pub bridge_address: Ipv4Addr,
    pub network_pool: Ipv4Addr,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Registry {
    pub authority: String,
    pub transport: RegistryTransport,
}
#[derive(Clone, Copy, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegistryTransport {
    Https,
    DevelopmentHttp,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        let Schema::V1 = self.schema;
        HostIdentity {
            provider_id: self.provider_id,
            engine_id: uuid::Uuid::from_u128(1),
            namespace: self.namespace.clone(),
        }
        .validate()?;
        ensure!(
            self.namespace
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "provider namespace must be a DNS label"
        );
        let authority = &self.registry.authority;
        let url = reqwest::Url::parse(&format!("https://{authority}/"))?;
        ensure!(
            !authority.is_empty()
                && authority.len() <= 253
                && authority
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".-:".contains(&b))
                && url.username().is_empty()
                && url.password().is_none()
                && url.host_str().is_some()
                && url.path() == "/",
            "registry requires one explicit DNS/IPv4 authority"
        );
        ensure!(
            !self.images.is_empty() && self.images.len() <= 64,
            "host image catalog must contain 1..64 images"
        );
        let mut images = BTreeSet::new();
        for image in &self.images {
            let Some((repository, digest)) = image.split_once("@sha256:") else {
                anyhow::bail!("host images require SHA-256 digests")
            };
            ensure!(
                repository.starts_with(&format!("{authority}/"))
                    && repository.len() > authority.len() + 1
                    && repository.len() <= 512
                    && repository.bytes().all(|b| b.is_ascii_lowercase()
                        || b.is_ascii_digit()
                        || b"._-/:".contains(&b))
                    && digest.len() == 64
                    && digest
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    && images.insert(image),
                "invalid or duplicate host image"
            );
        }
        ensure!(
            images.contains(&self.default_image),
            "default image must belong to the preload catalog"
        );
        ensure!(
            !self.templates.is_empty()
                && self.templates.len() <= 64
                && self.reserve_bytes >= 536870912,
            "invalid retained capacity catalog"
        );
        let mut fingerprints = BTreeSet::new();
        for template in &self.templates {
            ensure!(
                template.fingerprint.len() == 64
                    && template
                        .fingerprint
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    && fingerprints.insert(&template.fingerprint)
                    && (536870912..=274877906944).contains(&template.capacity_bytes)
                    && template.capacity_bytes.is_multiple_of(4096),
                "invalid retained template"
            );
        }
        ensure!(
            self.bridge_address.is_private()
                && self.bridge_address.octets()[3] == 1
                && self.network_pool.is_private()
                && self.network_pool.octets()[2..] == [0, 0]
                && self.bridge_address.octets()[..2] != self.network_pool.octets()[..2],
            "host requires distinct private bridge /24 and sandbox pool /16"
        );
        Ok(())
    }
    pub fn provider_config(&self) -> Result<String> {
        let image = serde_json::to_string(&self.default_image)?;
        Ok(format!(
            r#"[openshell]
version = 1
[openshell.gateway]
bind_address = "0.0.0.0:{PROVIDER_PORT}"
compute_drivers = ["docker"]
log_level = "warn"
ssh_session_ttl_secs = 3600
[openshell.gateway.mtls_auth]
enabled = true
user_common_names = ["veoveo-computers-worker"]
[openshell.gateway.gateway_jwt]
signing_key_path = "{RUN}/trust/jwt-key.pem"
public_key_path = "{RUN}/trust/jwt-public.pem"
kid_path = "{RUN}/trust/jwt-kid"
gateway_id = "{namespace}"
ttl_secs = 3600
[openshell.drivers.docker]
socket_path = "{SOCKET}"
host_gateway_ip = "{bridge}"
default_image = {image}
image_pull_policy = "Never"
sandbox_namespace = "{namespace}"
network_name = "{namespace}"
grpc_endpoint = "https://host.openshell.internal:{PROVIDER_PORT}"
supervisor_bin = "/usr/local/bin/openshell-sandbox"
guest_tls_ca = "{RUN}/trust/provider-ca.pem"
guest_tls_cert = "{RUN}/trust/guest.pem"
guest_tls_key = "{RUN}/trust/guest-key.pem"
sandbox_pids_limit = 256
enable_bind_mounts = false
"#,
            namespace = self.namespace,
            bridge = self.bridge_address
        ))
    }
    pub fn storage_config(&self) -> StorageConfig {
        StorageConfig {
            provider_id: self.provider_id,
            namespace: self.namespace.clone(),
            root: format!("{STATE}/retained").into(),
            reserve_bytes: self.reserve_bytes,
            templates: self.templates.clone(),
            docker_socket: SOCKET.into(),
            plugin_name: "veoveo-retained".into(),
            plugin_socket: PLUGIN.into(),
            listen: (Ipv4Addr::UNSPECIFIED, STORAGE_PORT).into(),
            tls: TlsConfig {
                worker_ca: format!("{RUN}/trust/storage-ca.pem").into(),
                certificate: format!("{RUN}/trust/storage-server.pem").into(),
                private_key: format!("{RUN}/trust/storage-server-key.pem").into(),
            },
        }
    }
}
