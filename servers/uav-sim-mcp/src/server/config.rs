use std::time::Duration;

use clap::{Parser, ValueEnum};
use secrecy::SecretString;
use url::Url;
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};
use veoveo_task_runtime::StoreAuthLevel;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(super) enum AdapterKind {
    Http,
    Fake,
}

#[derive(Parser)]
#[command(name = "uav-sim-mcp", about = "Durable UAV simulation MCP server")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8802)]
    pub(super) port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub(super) public_base_url: String,
    #[arg(long, default_value_t = false)]
    pub(super) allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_parser = parse_allowed_host)]
    pub(super) allowed_hosts: Vec<String>,
    #[arg(long, value_enum, default_value_t = AdapterKind::Http)]
    pub(super) adapter: AdapterKind,
    #[arg(
        long,
        env = "UAV_SIM_ADAPTER_URL",
        default_value = "http://127.0.0.1:8810/"
    )]
    pub(super) adapter_url: String,
    #[arg(long, env = "UAV_SIM_ADAPTER_TIMEOUT_SECONDS", default_value_t = 90)]
    pub(super) adapter_timeout_seconds: u64,
    #[arg(
        long,
        env = "UAV_SIM_ADAPTER_OPERATION_TIMEOUT_SECONDS",
        default_value_t = 3600
    )]
    pub(super) adapter_operation_timeout_seconds: u64,
    #[arg(
        long,
        env = "UAV_SIM_RECORDING_TENANT_KEY",
        default_value = "enterprise"
    )]
    pub(super) recording_tenant_key: String,
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

    pub(super) fn adapter_url(&self) -> anyhow::Result<Url> {
        let mut url = Url::parse(&self.adapter_url)?;
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        Ok(url)
    }

    pub(super) fn adapter_timeout(&self) -> anyhow::Result<Duration> {
        anyhow::ensure!(
            self.adapter_timeout_seconds > 0,
            "adapter timeout must be positive"
        );
        Ok(Duration::from_secs(self.adapter_timeout_seconds))
    }

    pub(super) fn adapter_operation_timeout(&self) -> anyhow::Result<Duration> {
        anyhow::ensure!(
            self.adapter_operation_timeout_seconds > 0,
            "adapter operation timeout must be positive"
        );
        Ok(Duration::from_secs(self.adapter_operation_timeout_seconds))
    }
}

fn parse_database_auth_level(value: &str) -> Result<StoreAuthLevel, String> {
    match value.parse::<StoreAuthLevel>() {
        Ok(StoreAuthLevel::Database) => Ok(StoreAuthLevel::Database),
        Ok(_) => Err("UAV simulation requires database-scoped SurrealDB credentials".to_owned()),
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
        .ok_or_else(|| "expected a host authority such as uav-sim-mcp:8802".to_owned())
}
