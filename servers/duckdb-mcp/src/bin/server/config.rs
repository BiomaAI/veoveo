use std::path::PathBuf;

use clap::Parser;
use secrecy::SecretString;
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};
use veoveo_task_runtime::StoreAuthLevel;

#[derive(Parser)]
#[command(name = "server", about = "DuckDB MCP server (streamable HTTP)")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8791)]
    pub(super) port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub(super) public_base_url: String,
    /// Root directory holding owner-scoped mutable database files.
    #[arg(long, default_value = "databases")]
    pub(super) database_dir: PathBuf,
    /// Root directory for per-request ingest/export file exchange.
    #[arg(long, default_value = "exchange")]
    pub(super) exchange_dir: PathBuf,
    /// DuckDB spill directory; must stay separate from the exchange root.
    #[arg(long, default_value = "spill")]
    pub(super) spill_dir: PathBuf,
    /// Preinstalled, signed DuckDB Spatial extension. The server loads this
    /// exact file before locking each DuckDB connection.
    #[arg(
        long,
        default_value = "/usr/local/lib/duckdb/extensions/spatial.duckdb_extension"
    )]
    pub(super) spatial_extension: PathBuf,
    /// Base URL of the shared artifact-plane service (e.g.
    /// http://artifact-service:8790). All artifact reads/writes go here.
    #[arg(long, default_value = "http://artifact-service:8790")]
    pub(super) artifact_service_url: String,
    #[arg(long, default_value_t = false)]
    pub(super) allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_name = "HOST", value_parser = parse_allowed_host)]
    pub(super) allowed_hosts: Vec<String>,
    /// Exact HTTPS hosts accepted for governed source materialization. Empty
    /// disables remote sources; inline and authorized artifact sources remain.
    #[arg(long = "allow-source-host", value_name = "HOST")]
    pub(super) allow_source_hosts: Vec<String>,
    #[arg(long, default_value_t = 268_435_456)]
    pub(super) max_source_bytes: u64,
    #[arg(long, default_value_t = 536_870_912)]
    pub(super) max_artifact_bytes: u64,
    /// Per-connection DuckDB memory limit.
    #[arg(long, default_value = "512MB")]
    pub(super) engine_memory_limit: String,
    /// Per-connection DuckDB thread cap.
    #[arg(long, default_value_t = 2)]
    pub(super) engine_threads: u32,
    /// Inline query output caps; larger results must spill to an artifact.
    #[arg(long, default_value_t = 1_000)]
    pub(super) max_inline_rows: u64,
    #[arg(long, default_value_t = 1_048_576)]
    pub(super) max_inline_bytes: u64,
    /// SQL execution timeout bounds in milliseconds.
    #[arg(long, default_value_t = 30_000)]
    pub(super) default_timeout_ms: u64,
    #[arg(long, default_value_t = 120_000)]
    pub(super) max_timeout_ms: u64,
    #[arg(long = "surreal-endpoint", env = "VEOVEO_SURREAL_ENDPOINT")]
    pub(super) surreal_endpoint: String,
    #[arg(long = "surreal-namespace", env = "VEOVEO_SURREAL_NAMESPACE")]
    pub(super) surreal_namespace: String,
    #[arg(long = "surreal-database", env = "VEOVEO_SURREAL_DATABASE")]
    pub(super) surreal_database: String,
    #[arg(
        long = "surreal-auth-level",
        env = "VEOVEO_SURREAL_AUTH_LEVEL",
        value_parser = parse_database_auth_level
    )]
    pub(super) surreal_auth_level: StoreAuthLevel,
    #[arg(long = "surreal-username", env = "VEOVEO_SURREAL_USERNAME")]
    pub(super) surreal_username: String,
    #[arg(
        long = "surreal-password",
        env = "VEOVEO_SURREAL_PASSWORD",
        hide_env_values = true,
        value_parser = parse_secret
    )]
    pub(super) surreal_password: SecretString,
    /// Public Ed25519 JWKS used to verify gateway identity assertions.
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    pub(super) internal_trust_jwks: String,
}

fn parse_database_auth_level(value: &str) -> Result<StoreAuthLevel, String> {
    match value.parse::<StoreAuthLevel>() {
        Ok(StoreAuthLevel::Database) => Ok(StoreAuthLevel::Database),
        Ok(_) => Err("duckdb requires database-scoped SurrealDB credentials".to_owned()),
        Err(error) => Err(error.to_string()),
    }
}

fn parse_secret(value: &str) -> Result<SecretString, String> {
    (!value.is_empty())
        .then(|| SecretString::from(value))
        .ok_or_else(|| "secret must not be empty".to_owned())
}

fn parse_allowed_host(value: &str) -> Result<String, String> {
    let value = value.trim();
    parse_allowed_host_authority(value)
        .map(|_| value.to_string())
        .ok_or_else(|| "expected a host authority such as duckdb-mcp:8791".to_string())
}

impl Args {
    pub(super) fn public_deployment(&self) -> anyhow::Result<PublicDeployment> {
        PublicDeployment::new(&self.public_base_url)
    }
}
