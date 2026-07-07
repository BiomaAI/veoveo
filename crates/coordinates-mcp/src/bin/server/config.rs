use clap::Parser;
use veoveo_mcp_contract::{PublicDeployment, parse_allowed_host_authority};

#[derive(Parser, Debug)]
#[command(name = "server", about = "Coordinates MCP server (streamable HTTP)")]
pub(super) struct Args {
    #[arg(long, default_value_t = 8793)]
    pub(super) port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    pub(super) public_base_url: String,
    /// Base URL of the shared artifact-plane service.
    #[arg(long, default_value = "http://artifact-service:8790")]
    pub(super) artifact_service_url: String,
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
        .ok_or_else(|| "expected a host authority such as coordinates-mcp:8793".to_string())
}

impl Args {
    pub(super) fn public_deployment(&self) -> anyhow::Result<PublicDeployment> {
        PublicDeployment::new(&self.public_base_url)
    }
}
