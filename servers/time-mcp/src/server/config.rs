use std::path::PathBuf;

use clap::Parser;
use secrecy::SecretString;
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};
use veoveo_task_runtime::StoreAuthLevel;

#[derive(Parser)]
#[command(name = "server", about = "Time MCP and administrative server")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8800)]
    pub port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub public_base_url: String,
    #[arg(long, default_value = "/usr/share/zoneinfo")]
    pub bootstrap_tzdb_dir: PathBuf,
    #[arg(long, default_value = "/usr/share/zoneinfo/leap-seconds.list")]
    pub bootstrap_leap_seconds_file: PathBuf,
    #[arg(
        long,
        default_value = "time-release-00000000-0000-7000-8000-000000000001"
    )]
    pub bootstrap_tzdb_release_id: String,
    #[arg(
        long,
        default_value = "time-release-00000000-0000-7000-8000-000000000002"
    )]
    pub bootstrap_leap_seconds_release_id: String,
    #[arg(long, default_value = "/var/lib/veoveo/time/acquisitions")]
    pub acquisition_scratch_root: PathBuf,
    #[arg(long, default_value = "/var/lib/veoveo/time/releases")]
    pub release_root: PathBuf,
    #[arg(long, default_value = "/usr/sbin/zic")]
    pub zic_executable: PathBuf,
    #[arg(long, default_value_t = 67_108_864)]
    pub maximum_source_bytes: u64,
    #[arg(long, default_value_t = 536_870_912)]
    pub maximum_expanded_bytes: u64,
    #[arg(long, default_value_t = 300)]
    pub acquisition_timeout_seconds: u64,
    #[arg(long)]
    pub ntpd_observation_socket: Option<PathBuf>,
    #[arg(long, default_value_t = 2)]
    pub clock_observation_timeout_seconds: u64,
    #[arg(long, default_value_t = false)]
    pub allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_name = "HOST", value_parser = parse_allowed_host)]
    pub allowed_hosts: Vec<String>,
    #[arg(long, default_value = "time:admin")]
    pub admin_scope: String,
    #[arg(long = "surreal-endpoint", env = "VEOVEO_SURREAL_ENDPOINT")]
    pub surreal_endpoint: String,
    #[arg(long = "surreal-namespace", env = "VEOVEO_SURREAL_NAMESPACE")]
    pub surreal_namespace: String,
    #[arg(long = "surreal-database", env = "VEOVEO_SURREAL_DATABASE")]
    pub surreal_database: String,
    #[arg(long = "surreal-auth-level", env = "VEOVEO_SURREAL_AUTH_LEVEL", value_parser = parse_database_auth_level)]
    pub surreal_auth_level: StoreAuthLevel,
    #[arg(long = "surreal-username", env = "VEOVEO_SURREAL_USERNAME")]
    pub surreal_username: String,
    #[arg(long = "surreal-password", env = "VEOVEO_SURREAL_PASSWORD", hide_env_values = true, value_parser = parse_secret)]
    pub surreal_password: SecretString,
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    pub internal_trust_jwks: String,
}

impl Args {
    pub fn public_deployment(&self) -> anyhow::Result<PublicDeployment> {
        PublicDeployment::new(&self.public_base_url)
    }
}

fn parse_database_auth_level(value: &str) -> Result<StoreAuthLevel, String> {
    match value.parse::<StoreAuthLevel>() {
        Ok(StoreAuthLevel::Database) => Ok(StoreAuthLevel::Database),
        Ok(_) => Err("time requires database-scoped SurrealDB credentials".to_owned()),
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
        .ok_or_else(|| "expected a host authority such as time-mcp:8800".to_owned())
}
