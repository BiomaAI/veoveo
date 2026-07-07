//! Environment configuration for the artifact service.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use object_store::ObjectStore;
use veoveo_mcp_contract::TokenIssuer;
use veoveo_mcp_contract::gateway::ServerSlug;
use veoveo_mcp_contract::internal_auth::{GATEWAY_INTERNAL_TOKEN_ISSUER, InternalTokenSecret};

use crate::crypto::MasterKey;

/// Where artifact bytes live.
#[derive(Debug, Clone)]
pub enum ObjectStoreConfig {
    /// In-process store, for local/dev only. Not durable.
    Memory,
    /// S3-compatible store (RustFS locally, S3/R2/MinIO in production).
    S3 {
        endpoint: Option<String>,
        bucket: String,
        region: String,
        access_key_id: String,
        secret_access_key: String,
        allow_http: bool,
    },
}

impl ObjectStoreConfig {
    pub fn build(&self) -> anyhow::Result<Arc<dyn ObjectStore>> {
        match self {
            ObjectStoreConfig::Memory => Ok(Arc::new(object_store::memory::InMemory::new())),
            ObjectStoreConfig::S3 {
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
                    .with_secret_access_key(secret_access_key)
                    .with_allow_http(*allow_http);
                if let Some(endpoint) = endpoint {
                    builder = builder.with_endpoint(endpoint);
                }
                let store = builder.build().context("building S3 object store")?;
                Ok(Arc::new(store))
            }
        }
    }
}

/// Fully-resolved service configuration.
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub db_max_connections: u32,
    pub internal_token_issuer: TokenIssuer,
    pub allowed_audiences: Vec<ServerSlug>,
    pub internal_token_secret: InternalTokenSecret,
    pub master_key: MasterKey,
    pub object_store: ObjectStoreConfig,
}

fn env(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind: SocketAddr = env_or("ARTIFACT_SERVICE_BIND", "0.0.0.0:8790")
            .parse()
            .context("parsing ARTIFACT_SERVICE_BIND")?;

        let database_url = env("DATABASE_URL")?;
        let db_max_connections = env_or("ARTIFACT_DB_MAX_CONNECTIONS", "5")
            .parse()
            .context("parsing ARTIFACT_DB_MAX_CONNECTIONS")?;

        let internal_token_issuer = TokenIssuer::new(env_or(
            "INTERNAL_TOKEN_ISSUER",
            GATEWAY_INTERNAL_TOKEN_ISSUER,
        ))
        .map_err(|e| anyhow!("invalid INTERNAL_TOKEN_ISSUER: {e}"))?;

        let allowed_audiences = env_or(
            "ARTIFACT_ALLOWED_AUDIENCES",
            "media,timeseries,optimization,duckdb",
        );
        let allowed_audiences = allowed_audiences
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| ServerSlug::new(s).map_err(|e| anyhow!("invalid audience `{s}`: {e}")))
            .collect::<anyhow::Result<Vec<_>>>()?;
        if allowed_audiences.is_empty() {
            return Err(anyhow!(
                "ARTIFACT_ALLOWED_AUDIENCES must list at least one server"
            ));
        }

        let internal_token_secret = InternalTokenSecret::new(env("INTERNAL_TOKEN_SECRET")?)
            .map_err(|e| anyhow!("invalid INTERNAL_TOKEN_SECRET: {e}"))?;

        let master_key_hex = env("ARTIFACT_MASTER_KEY")?;
        let master_key_bytes =
            hex::decode(master_key_hex.trim()).context("ARTIFACT_MASTER_KEY must be hex")?;
        let master_key = MasterKey::new(master_key_bytes)
            .map_err(|e| anyhow!("invalid ARTIFACT_MASTER_KEY: {e}"))?;

        let object_store = match env_or("ARTIFACT_STORE", "s3").as_str() {
            "memory" => ObjectStoreConfig::Memory,
            "s3" => ObjectStoreConfig::S3 {
                endpoint: std::env::var("ARTIFACT_S3_ENDPOINT").ok(),
                bucket: env("ARTIFACT_S3_BUCKET")?,
                region: env_or("ARTIFACT_S3_REGION", "us-east-1"),
                access_key_id: env("ARTIFACT_S3_ACCESS_KEY_ID")?,
                secret_access_key: env("ARTIFACT_S3_SECRET_ACCESS_KEY")?,
                allow_http: env_or("ARTIFACT_S3_ALLOW_HTTP", "false") == "true",
            },
            other => return Err(anyhow!("unknown ARTIFACT_STORE `{other}` (memory|s3)")),
        };

        Ok(Self {
            bind,
            database_url,
            db_max_connections,
            internal_token_issuer,
            allowed_audiences,
            internal_token_secret,
            master_key,
            object_store,
        })
    }
}
