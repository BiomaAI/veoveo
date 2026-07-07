use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};

#[derive(Parser, Debug)]
#[command(name = "server", about = "DuckDB MCP server (streamable HTTP)")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8791)]
    pub(super) port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub(super) public_base_url: String,
    #[arg(long, default_value = "state.duckdb")]
    pub(super) state_db: PathBuf,
    /// Root directory holding owner-scoped mutable database files.
    #[arg(long, default_value = "databases")]
    pub(super) database_dir: PathBuf,
    /// Root directory for per-request ingest/export file exchange.
    #[arg(long, default_value = "exchange")]
    pub(super) exchange_dir: PathBuf,
    /// DuckDB spill directory; must stay separate from the exchange root.
    #[arg(long, default_value = "spill")]
    pub(super) spill_dir: PathBuf,
    #[arg(long, default_value = "s3-compatible")]
    pub(super) artifact_store: ArtifactStoreBackend,
    #[arg(long, default_value = "http://localhost:9000")]
    pub(super) artifact_endpoint: String,
    #[arg(long, default_value = "duckdb-artifacts")]
    pub(super) artifact_bucket: String,
    #[arg(long, default_value = "us-east-1")]
    pub(super) artifact_region: String,
    #[arg(long, default_value_t = false)]
    pub(super) artifact_allow_http: bool,
    #[arg(long, default_value_t = false)]
    pub(super) allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_name = "HOST", value_parser = parse_allowed_host)]
    pub(super) allowed_hosts: Vec<String>,
    /// HTTPS hosts the server may fetch ingest source URIs from. Empty means
    /// inline sources only.
    #[arg(long = "allow-ingest-host", value_name = "HOST")]
    pub(super) allow_ingest_hosts: Vec<String>,
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
    #[arg(long, env = "VEOVEO_INTERNAL_TOKEN_SECRET", hide_env_values = true)]
    pub(super) internal_token_secret: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub(super) enum ArtifactStoreBackend {
    S3Compatible,
    Memory,
}
