use std::path::PathBuf;

use clap::Parser;
use secrecy::SecretString;
use veoveo_mcp_contract::parse_allowed_host_authority;

#[derive(Parser)]
#[command(name = "server", about = "Governed Recording MCP server")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8796)]
    pub(super) port: u16,
    #[arg(long, env = "RECORDING_SPOOL_DIR")]
    pub(super) spool_dir: PathBuf,
    #[arg(
        long,
        env = "ARTIFACT_SERVICE_URL",
        default_value = "http://artifact-service:8790"
    )]
    pub(super) artifact_service_url: String,
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    pub(super) internal_trust_jwks: String,
    #[arg(long, env = "VEOVEO_SURREAL_ENDPOINT")]
    pub(super) surreal_endpoint: String,
    #[arg(long, env = "VEOVEO_SURREAL_NAMESPACE")]
    pub(super) surreal_namespace: String,
    #[arg(long, env = "VEOVEO_SURREAL_DATABASE")]
    pub(super) surreal_database: String,
    #[arg(long, env = "VEOVEO_SURREAL_USERNAME")]
    pub(super) surreal_username: String,
    #[arg(
        long,
        env = "VEOVEO_SURREAL_PASSWORD",
        hide_env_values = true,
        value_parser = parse_secret
    )]
    pub(super) surreal_password: SecretString,
    #[arg(long, default_value_t = false)]
    pub(super) allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_parser = parse_allowed_host)]
    pub(super) allowed_hosts: Vec<String>,
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
        .ok_or_else(|| "expected a host authority such as recording-mcp:8796".to_owned())
}
