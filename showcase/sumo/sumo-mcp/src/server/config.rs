use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, ValueEnum};
use secrecy::SecretString;
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};
use veoveo_task_runtime::StoreAuthLevel;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(super) enum DriverKind {
    Traci,
    Fake,
}

#[derive(Parser)]
#[command(name = "sumo-mcp", about = "Durable SUMO traffic-world MCP server")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8795)]
    pub(super) port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub(super) public_base_url: String,
    #[arg(long, default_value_t = false)]
    pub(super) allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_parser = parse_allowed_host)]
    pub(super) allowed_hosts: Vec<String>,
    #[arg(long, value_enum, default_value_t = DriverKind::Traci)]
    pub(super) driver: DriverKind,
    #[arg(long, env = "SUMO_HOST", default_value = "sumo")]
    pub(super) sumo_host: String,
    #[arg(long, env = "SUMO_PORT", default_value_t = 8813)]
    pub(super) sumo_port: u16,
    #[arg(long, env = "SUMO_SCENARIO", default_value = "sumo")]
    pub(super) scenario: String,
    #[arg(long, env = "SUMO_MAX_VEHICLES", default_value_t = 800)]
    pub(super) max_vehicles: usize,
    #[arg(long, env = "SUMO_CONNECT_RETRIES", default_value_t = 180)]
    pub(super) connect_retries: u32,
    #[arg(long, env = "SUMO_FAKE_VEHICLES", default_value_t = 12)]
    pub(super) fake_vehicles: usize,
    #[arg(long, env = "SUMO_FAKE_SEED", default_value_t = 1)]
    pub(super) fake_seed: u64,
    #[arg(long, default_value = "rerun+http://127.0.0.1:9876/proxy")]
    pub(super) recording_proxy: String,
    #[arg(long, default_value = "sumo-live")]
    pub(super) recording: String,
    #[arg(long, default_value_t = 100)]
    pub(super) step_interval_ms: u64,
    #[arg(long, default_value = "/var/lib/veoveo/sumo")]
    pub(super) work_dir: PathBuf,
    #[arg(long, default_value = "http://artifact-service:8790")]
    pub(super) artifact_service_url: String,
    #[arg(long, default_value_t = 268_435_456)]
    pub(super) max_artifact_bytes: u64,
    #[arg(long, default_value = "netgenerate")]
    pub(super) netgenerate_bin: PathBuf,
    #[arg(long, default_value = "duarouter")]
    pub(super) duarouter_bin: PathBuf,
    #[arg(long, default_value = "tlsCoordinator.py")]
    pub(super) tls_coordinator_bin: PathBuf,
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

    pub(super) fn step_interval(&self) -> anyhow::Result<Duration> {
        anyhow::ensure!(self.step_interval_ms > 0, "step interval must be positive");
        Ok(Duration::from_millis(self.step_interval_ms))
    }
}

fn parse_database_auth_level(value: &str) -> Result<StoreAuthLevel, String> {
    match value.parse::<StoreAuthLevel>() {
        Ok(StoreAuthLevel::Database) => Ok(StoreAuthLevel::Database),
        Ok(_) => Err("SUMO requires database-scoped SurrealDB credentials".to_owned()),
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
        .map(|_| value.to_owned())
        .ok_or_else(|| "expected a host authority such as sumo-mcp:8795".to_owned())
}
