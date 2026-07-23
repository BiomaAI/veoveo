use std::time::Duration;

use clap::{Parser, ValueEnum};
use secrecy::SecretString;
use url::Url;
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};
use veoveo_task_runtime::StoreAuthLevel;

use crate::contract::LiveStreamEndpoint;

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
    #[arg(
        long,
        env = "UAV_SIM_LIVE_STREAM_SIGNALING_URL",
        default_value = "ws://127.0.0.1:49101/webrtc"
    )]
    pub(super) live_stream_signaling_url: String,
    #[arg(
        long,
        env = "UAV_SIM_LIVE_STREAM_MEDIA_SERVER",
        default_value = "127.0.0.1"
    )]
    pub(super) live_stream_media_server: String,
    #[arg(long, env = "UAV_SIM_LIVE_STREAM_MEDIA_PORT", default_value_t = 47998)]
    pub(super) live_stream_media_port: u16,
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

    pub(super) fn live_stream_endpoint(&self) -> anyhow::Result<LiveStreamEndpoint> {
        let url = Url::parse(&self.live_stream_signaling_url)?;
        anyhow::ensure!(
            matches!(url.scheme(), "ws" | "wss"),
            "live-stream signaling URL must use ws or wss"
        );
        anyhow::ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "live-stream signaling URL must not contain credentials, a query, or fragment"
        );
        let signaling_server = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("live-stream signaling URL requires a host"))?
            .to_owned();
        let signaling_port = url
            .port_or_known_default()
            .ok_or_else(|| anyhow::anyhow!("live-stream signaling URL requires a port"))?;
        let signaling_path = url.path().to_owned();
        anyhow::ensure!(
            signaling_path.starts_with('/') && signaling_path.len() > 1,
            "live-stream signaling URL requires a non-root path"
        );
        anyhow::ensure!(
            !self.live_stream_media_server.trim().is_empty()
                && !self.live_stream_media_server.contains('/')
                && !self.live_stream_media_server.contains("://"),
            "live-stream media server must be a hostname or IP address"
        );
        Ok(LiveStreamEndpoint {
            signaling_server,
            signaling_port,
            signaling_path,
            media_server: self.live_stream_media_server.clone(),
            media_port: self.live_stream_media_port,
            force_wss: url.scheme() == "wss",
        })
    }

    pub(super) fn live_stream_connect_origin(&self) -> anyhow::Result<String> {
        let url = Url::parse(&self.live_stream_signaling_url)?;
        anyhow::ensure!(
            url.host_str().is_some(),
            "live-stream signaling URL requires a host"
        );
        Ok(url[..url::Position::BeforePath]
            .trim_end_matches('/')
            .to_owned())
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
