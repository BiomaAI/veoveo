use std::path::PathBuf;

use clap::Parser;
use secrecy::SecretString;
use url::Url;
use veoveo_mcp_contract::parse_allowed_host_authority;
use veoveo_recording_hub::ClientAssertionAlgorithm;

#[derive(Parser)]
#[command(name = "server", about = "Governed Recording MCP server")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8796)]
    pub(super) port: u16,
    #[arg(long, env = "RECORDING_SPOOL_DIR")]
    pub(super) spool_dir: PathBuf,
    #[arg(
        long,
        env = "RECORDING_CATALOG_CACHE_DIR",
        default_value = "/recording-cache"
    )]
    pub(super) catalog_cache_dir: PathBuf,
    #[arg(
        long,
        env = "RECORDING_CATALOG_CACHE_MANAGED_BYTES",
        default_value_t = 8 * 1024 * 1024 * 1024_u64
    )]
    pub(super) catalog_cache_managed_bytes: u64,
    #[arg(
        long,
        env = "RECORDING_CATALOG_CACHE_MINIMUM_FREE_BYTES",
        default_value_t = 1024 * 1024 * 1024_u64
    )]
    pub(super) catalog_cache_minimum_free_bytes: u64,
    #[arg(
        long,
        env = "RECORDING_PROJECTION_SCRATCH_BYTES",
        default_value_t = 96 * 1024 * 1024_u64
    )]
    pub(super) projection_scratch_bytes: u64,
    #[arg(
        long,
        env = "RECORDING_PROJECTION_MINIMUM_FREE_BYTES",
        default_value_t = 1024 * 1024 * 1024_u64
    )]
    pub(super) projection_minimum_free_bytes: u64,
    #[arg(
        long,
        env = "RECORDING_PROJECTION_CONCURRENCY",
        default_value_t = 2,
        value_parser = clap::value_parser!(u8).range(1..=2)
    )]
    pub(super) projection_concurrency: u8,
    #[arg(
        long,
        env = "RECORDING_PROJECTION_DEADLINE_MS",
        default_value_t = 15_000,
        value_parser = clap::value_parser!(u64).range(1..=15_000)
    )]
    pub(super) projection_deadline_ms: u64,
    #[arg(long, env = "VEOVEO_GATEWAY_URL")]
    pub(super) gateway_url: Url,
    #[arg(long, env = "VEOVEO_RECORDING_GATEWAY_TRANSPORT_URL")]
    pub(super) gateway_transport_url: Option<Url>,
    #[arg(long, env = "VEOVEO_RECORDING_PUBLICATION_RESOURCE")]
    pub(super) publication_protected_resource: Url,
    #[arg(
        long,
        env = "VEOVEO_RECORDING_PUBLICATION_PROFILE",
        default_value = "recording-publish"
    )]
    pub(super) publication_profile: String,
    #[arg(
        long,
        env = "VEOVEO_RECORDING_MCP_PUBLISHER_CLIENT_ID",
        default_value = "recording-mcp-publisher"
    )]
    pub(super) publication_client_id: String,
    #[arg(long, env = "VEOVEO_RECORDING_MCP_PUBLISHER_PRIVATE_KEY_PEM_FILE")]
    pub(super) publication_private_key_pem_file: PathBuf,
    #[arg(long, env = "VEOVEO_RECORDING_MCP_PUBLISHER_KEY_ID")]
    pub(super) publication_key_id: String,
    #[arg(
        long,
        env = "VEOVEO_RECORDING_MCP_PUBLISHER_SIGNING_ALGORITHM",
        default_value = "rs256"
    )]
    pub(super) publication_signing_algorithm: ClientAssertionAlgorithm,
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
    #[arg(
        long,
        env = "RECORDING_LIVE_HISTORY_SECONDS",
        default_value_t = 60,
        value_parser = clap::value_parser!(u64).range(1..=3600)
    )]
    pub(super) live_history_seconds: u64,
    #[arg(
        long,
        env = "RECORDING_PLAYBACK_TOKEN_KEY",
        hide_env_values = true,
        value_parser = parse_secret
    )]
    pub(super) playback_token_key: SecretString,
    #[arg(long, env = "RECORDING_PLAYBACK_PUBLIC_URL")]
    pub(super) playback_public_url: String,
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
