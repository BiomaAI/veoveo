use std::path::PathBuf;

use clap::Parser;
use secrecy::SecretString;
use url::Url;
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};
use veoveo_task_runtime::StoreAuthLevel;

#[derive(Parser)]
#[command(name = "server", about = "Map MCP and administrative server")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8799)]
    pub port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub public_base_url: String,
    #[arg(long, default_value = "http://artifact-service:8790")]
    pub artifact_service_url: String,
    #[arg(long, default_value = "/var/lib/veoveo/map/catalog.duckdb")]
    pub map_database: PathBuf,
    #[arg(long, default_value = "/var/lib/veoveo/map/spill")]
    pub duckdb_spill_dir: PathBuf,
    #[arg(
        long,
        default_value = "/usr/local/lib/duckdb/extensions/spatial.duckdb_extension"
    )]
    pub spatial_extension: PathBuf,
    #[arg(long, default_value = "1GB")]
    pub duckdb_memory_limit: String,
    #[arg(long, default_value_t = 4)]
    pub duckdb_threads: u32,
    #[arg(long, default_value = "http://127.0.0.1:8002/")]
    pub valhalla_url: Url,
    #[arg(long, default_value = "/usr/local/bin/valhalla_service")]
    pub valhalla_executable: PathBuf,
    #[arg(long, default_value = "/etc/veoveo/map/valhalla.json")]
    pub valhalla_config: PathBuf,
    #[arg(long, default_value_t = 2)]
    pub valhalla_concurrency: u16,
    #[arg(long, default_value_t = 90)]
    pub valhalla_startup_timeout_seconds: u64,
    #[arg(long, default_value_t = 30)]
    pub valhalla_timeout_seconds: u64,
    #[arg(long, default_value = "/usr/bin/python3")]
    pub helper_python: PathBuf,
    #[arg(long, default_value = "map_data")]
    pub helper_module: String,
    #[arg(long, default_value = "/var/lib/veoveo/map/acquisitions")]
    pub acquisition_scratch_root: PathBuf,
    #[arg(long, default_value = "/var/lib/veoveo/map/releases")]
    pub release_root: PathBuf,
    #[arg(long, default_value = "/var/lib/veoveo/map/valhalla/active")]
    pub valhalla_active_dir: PathBuf,
    #[arg(long, default_value = "/data/map-sources")]
    pub source_mount_root: PathBuf,
    #[arg(long, default_value = "/run/secrets/map")]
    pub source_secret_root: PathBuf,
    /// Installation-owned source catalog to register once when entries are absent.
    #[arg(long)]
    pub bootstrap_catalog: Option<PathBuf>,
    #[arg(long, default_value_t = 268_435_456)]
    pub max_artifact_bytes: u64,
    #[arg(long, default_value_t = 17_179_869_184)]
    pub max_routing_expanded_bytes: u64,
    #[arg(long, default_value_t = false)]
    pub allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_name = "HOST", value_parser = parse_allowed_host)]
    pub allowed_hosts: Vec<String>,
    #[arg(long, default_value = "map:admin")]
    pub admin_scope: String,
    #[arg(long = "surreal-endpoint", env = "VEOVEO_SURREAL_ENDPOINT")]
    pub surreal_endpoint: String,
    #[arg(long = "surreal-namespace", env = "VEOVEO_SURREAL_NAMESPACE")]
    pub surreal_namespace: String,
    #[arg(long = "surreal-database", env = "VEOVEO_SURREAL_DATABASE")]
    pub surreal_database: String,
    #[arg(
        long = "surreal-auth-level",
        env = "VEOVEO_SURREAL_AUTH_LEVEL",
        value_parser = parse_database_auth_level
    )]
    pub surreal_auth_level: StoreAuthLevel,
    #[arg(long = "surreal-username", env = "VEOVEO_SURREAL_USERNAME")]
    pub surreal_username: String,
    #[arg(
        long = "surreal-password",
        env = "VEOVEO_SURREAL_PASSWORD",
        hide_env_values = true,
        value_parser = parse_secret
    )]
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
        Ok(_) => Err("map requires database-scoped SurrealDB credentials".to_owned()),
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
        .ok_or_else(|| "expected a host authority such as map-mcp:8799".to_owned())
}
