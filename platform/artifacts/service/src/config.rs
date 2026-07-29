//! Environment configuration for the artifact service.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use object_store::ObjectStore;
use secrecy::{ExposeSecret, SecretString};
use veoveo_mcp_contract::gateway::ServerSlug;
use veoveo_mcp_contract::internal_auth::GATEWAY_INTERNAL_TOKEN_ISSUER;
use veoveo_mcp_contract::{GatewayInternalTrustBundle, TokenIssuer};
use veoveo_platform_store::{StoreAuthLevel, StoreConfig, StoreCredentials};

use crate::store::ArtifactObjectStore;

#[derive(Clone)]
pub enum ObjectStoreConfig {
    Memory,
    Filesystem {
        root: PathBuf,
    },
    S3 {
        endpoint: Option<String>,
        bucket: String,
        region: String,
        access_key_id: String,
        secret_access_key: SecretString,
        allow_http: bool,
    },
}

impl ObjectStoreConfig {
    pub fn build(&self) -> anyhow::Result<ArtifactObjectStore> {
        match self {
            Self::Memory => Ok(ArtifactObjectStore::new(Arc::new(
                object_store::memory::InMemory::new(),
            ))),
            Self::Filesystem { root } => {
                std::fs::create_dir_all(root)
                    .with_context(|| format!("creating artifact store {}", root.display()))?;
                let local = object_store::local::LocalFileSystem::new_with_prefix(root)
                    .context("building local artifact store")?;
                Ok(ArtifactObjectStore::new(Arc::new(local)))
            }
            Self::S3 {
                endpoint,
                bucket,
                region,
                access_key_id,
                secret_access_key,
                allow_http,
            } => {
                let mut builder = object_store::aws::AmazonS3Builder::new()
                    .with_bucket_name(bucket)
                    .with_region(region)
                    .with_access_key_id(access_key_id)
                    .with_secret_access_key(secret_access_key.expose_secret())
                    .with_allow_http(*allow_http);
                if let Some(endpoint) = endpoint {
                    builder = builder.with_endpoint(endpoint);
                }
                let store = Arc::new(builder.build().context("building S3 artifact store")?);
                let object_store: Arc<dyn ObjectStore> = store;
                Ok(ArtifactObjectStore::new(object_store))
            }
        }
    }
}

impl std::fmt::Debug for ObjectStoreConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory => formatter.write_str("Memory"),
            Self::Filesystem { root } => formatter
                .debug_struct("Filesystem")
                .field("root", root)
                .finish(),
            Self::S3 {
                endpoint,
                bucket,
                region,
                access_key_id,
                secret_access_key: _,
                allow_http,
            } => formatter
                .debug_struct("S3")
                .field("endpoint", endpoint)
                .field("bucket", bucket)
                .field("region", region)
                .field("access_key_id", access_key_id)
                .field("secret_access_key", &"[REDACTED]")
                .field("allow_http", allow_http)
                .finish(),
        }
    }
}

pub struct Config {
    pub bind: SocketAddr,
    pub public_base_url: String,
    pub platform_store: StoreConfig,
    pub internal_token_issuer: TokenIssuer,
    pub allowed_audiences: Vec<ServerSlug>,
    pub internal_trust_bundle: GatewayInternalTrustBundle,
    pub object_store: ObjectStoreConfig,
    pub max_internal_read_bytes: u64,
}

fn env(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn positive_u64(key: &str, default: &str) -> anyhow::Result<u64> {
    let value = env_or(key, default)
        .parse::<u64>()
        .with_context(|| format!("parsing {key}"))?;
    if value == 0 {
        return Err(anyhow!("{key} must be greater than zero"));
    }
    Ok(value)
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind: SocketAddr = env_or("ARTIFACT_SERVICE_BIND", "0.0.0.0:8790")
            .parse()
            .context("parsing ARTIFACT_SERVICE_BIND")?;
        let public_base_url = env("ARTIFACT_PUBLIC_BASE_URL")?
            .trim_end_matches('/')
            .to_owned();
        let parsed_base = url::Url::parse(&public_base_url)
            .context("ARTIFACT_PUBLIC_BASE_URL must be an absolute URL")?;
        if !matches!(parsed_base.scheme(), "http" | "https") || parsed_base.host_str().is_none() {
            return Err(anyhow!(
                "ARTIFACT_PUBLIC_BASE_URL must use http or https and include a host"
            ));
        }

        let auth_level = env("VEOVEO_SURREAL_AUTH_LEVEL")?
            .parse::<StoreAuthLevel>()
            .context("parsing VEOVEO_SURREAL_AUTH_LEVEL")?;
        if auth_level != StoreAuthLevel::Database {
            return Err(anyhow!(
                "artifact-service requires VEOVEO_SURREAL_AUTH_LEVEL=database"
            ));
        }
        let credentials = StoreCredentials::new(
            auth_level,
            env("VEOVEO_SURREAL_USERNAME")?,
            env("VEOVEO_SURREAL_PASSWORD")?,
        );
        let platform_store = StoreConfig::builder(
            env("VEOVEO_SURREAL_ENDPOINT")?,
            env("VEOVEO_SURREAL_NAMESPACE")?,
            env("VEOVEO_SURREAL_DATABASE")?,
            credentials,
        )
        .migrate_on_connect(false)
        .build()
        .context("building SurrealDB configuration")?;

        let internal_token_issuer = TokenIssuer::new(env_or(
            "INTERNAL_TOKEN_ISSUER",
            GATEWAY_INTERNAL_TOKEN_ISSUER,
        ))
        .map_err(|error| anyhow!("invalid INTERNAL_TOKEN_ISSUER: {error}"))?;
        let allowed_audiences = env_or(
            "ARTIFACT_ALLOWED_AUDIENCES",
            "media,timeseries,optimization,duckdb",
        )
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            ServerSlug::new(value)
                .map_err(|error| anyhow!("invalid artifact audience `{value}`: {error}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
        if allowed_audiences.is_empty() {
            return Err(anyhow!("ARTIFACT_ALLOWED_AUDIENCES must not be empty"));
        }
        let internal_trust_bundle =
            GatewayInternalTrustBundle::from_json(&env("VEOVEO_INTERNAL_TRUST_JWKS")?)
                .map_err(|error| anyhow!("invalid VEOVEO_INTERNAL_TRUST_JWKS: {error}"))?;

        let object_store = match env_or("ARTIFACT_STORE", "s3").as_str() {
            "memory" => ObjectStoreConfig::Memory,
            "filesystem" => ObjectStoreConfig::Filesystem {
                root: PathBuf::from(env("ARTIFACT_FILESYSTEM_ROOT")?),
            },
            "s3" => ObjectStoreConfig::S3 {
                endpoint: std::env::var("ARTIFACT_S3_ENDPOINT").ok(),
                bucket: env("ARTIFACT_S3_BUCKET")?,
                region: env_or("ARTIFACT_S3_REGION", "us-east-1"),
                access_key_id: env("ARTIFACT_S3_ACCESS_KEY_ID")?,
                secret_access_key: SecretString::from(env("ARTIFACT_S3_SECRET_ACCESS_KEY")?),
                allow_http: env_or("ARTIFACT_S3_ALLOW_HTTP", "false") == "true",
            },
            other => {
                return Err(anyhow!(
                    "unknown ARTIFACT_STORE `{other}` (memory|filesystem|s3)"
                ));
            }
        };

        Ok(Self {
            bind,
            public_base_url,
            platform_store,
            internal_token_issuer,
            allowed_audiences,
            internal_trust_bundle,
            object_store,
            max_internal_read_bytes: positive_u64("ARTIFACT_INTERNAL_READ_MAX_BYTES", "67108864")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s3_debug_redacts_credentials_and_has_no_public_endpoint() {
        let config = ObjectStoreConfig::S3 {
            endpoint: Some("http://rustfs:9000".into()),
            bucket: "artifacts".into(),
            region: "us-east-1".into(),
            access_key_id: "test-access".into(),
            secret_access_key: SecretString::from("test-secret"),
            allow_http: true,
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("test-secret"));
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("public"));
    }
}
