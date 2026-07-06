use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};

#[derive(Parser, Debug)]
#[command(name = "server", about = "Timeseries MCP server (streamable HTTP)")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8788)]
    pub(super) port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub(super) public_base_url: String,
    #[arg(long, default_value = "state.duckdb")]
    pub(super) state_db: PathBuf,
    #[arg(long, default_value = "s3-compatible")]
    pub(super) artifact_store: ArtifactStoreBackend,
    #[arg(long, default_value = "http://localhost:9000")]
    pub(super) artifact_endpoint: String,
    #[arg(long, default_value = "timeseries-artifacts")]
    pub(super) artifact_bucket: String,
    #[arg(long, default_value = "us-east-1")]
    pub(super) artifact_region: String,
    #[arg(long, default_value_t = false)]
    pub(super) artifact_allow_http: bool,
    #[arg(long, default_value_t = false)]
    pub(super) allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_name = "HOST", value_parser = parse_allowed_host)]
    pub(super) allowed_hosts: Vec<String>,
    #[arg(long, env = "VEOVEO_INTERNAL_TOKEN_SECRET", hide_env_values = true)]
    pub(super) internal_token_secret: String,
}

fn parse_allowed_host(value: &str) -> Result<String, String> {
    let value = value.trim();
    parse_allowed_host_authority(value)
        .map(|_| value.to_string())
        .ok_or_else(|| "expected a host authority such as timeseries-mcp:8788".to_string())
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
