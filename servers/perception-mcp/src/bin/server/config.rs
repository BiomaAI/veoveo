use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use secrecy::SecretString;
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};
use veoveo_task_runtime::StoreAuthLevel;

#[derive(Parser)]
#[command(name = "server", about = "Perception MCP server (streamable HTTP)")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8797)]
    pub(super) port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub(super) public_base_url: String,
    #[arg(long, default_value = "http://artifact-service:8790")]
    pub(super) artifact_service_url: String,
    #[arg(long, default_value = "/recordings")]
    pub(super) spool_dir: PathBuf,
    #[arg(long, default_value = "/etc/veoveo/perception/catalog.json")]
    pub(super) pipeline_catalog: PathBuf,
    #[arg(long, default_value = "/usr/local/bin/perception-deepstream-runner")]
    pub(super) deepstream_runner: PathBuf,
    #[arg(long, default_value_t = 3_600)]
    runner_timeout_s: u64,
    #[arg(long, default_value_t = 100_000)]
    pub(super) max_video_samples: usize,
    #[arg(long, default_value_t = 2_147_483_648)]
    pub(super) max_encoded_video_bytes: u64,
    #[arg(long, default_value_t = 8_589_934_592)]
    pub(super) max_segment_bytes: u64,
    #[arg(long, default_value_t = 100_000)]
    pub(super) max_result_frames: usize,
    #[arg(long, default_value_t = 10_000)]
    pub(super) max_detections_per_frame: usize,
    #[arg(long, default_value_t = 268_435_456)]
    pub(super) max_runner_response_bytes: u64,
    #[arg(long, default_value_t = 16_777_216)]
    pub(super) max_inline_resource_bytes: u64,
    #[arg(long, default_value_t = 1)]
    pub(super) max_concurrent_jobs: usize,
    #[arg(long, default_value_t = 2_147_483_648)]
    pub(super) max_artifact_bytes: u64,
    #[arg(long, default_value_t = false)]
    pub(super) allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_name = "HOST", value_parser = parse_allowed_host)]
    pub(super) allowed_hosts: Vec<String>,
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
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    pub(super) internal_trust_jwks: String,
}

impl Args {
    pub(super) fn public_deployment(&self) -> anyhow::Result<PublicDeployment> {
        PublicDeployment::new(&self.public_base_url)
    }

    pub(super) fn runner_timeout(&self) -> Duration {
        Duration::from_secs(self.runner_timeout_s)
    }
}

fn parse_allowed_host(value: &str) -> Result<String, String> {
    let value = value.trim();
    parse_allowed_host_authority(value)
        .map(|_| value.to_owned())
        .ok_or_else(|| "expected a host authority such as perception-mcp:8797".to_owned())
}

fn parse_database_auth_level(value: &str) -> Result<StoreAuthLevel, String> {
    match value.parse::<StoreAuthLevel>() {
        Ok(StoreAuthLevel::Database) => Ok(StoreAuthLevel::Database),
        Ok(_) => Err("perception requires database-scoped SurrealDB credentials".to_owned()),
        Err(error) => Err(error.to_string()),
    }
}

fn parse_secret(value: &str) -> Result<SecretString, String> {
    (!value.is_empty())
        .then(|| SecretString::from(value))
        .ok_or_else(|| "secret must not be empty".to_owned())
}
