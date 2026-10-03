//! Glossary MCP server: the Rust template for a hosted Veoveo MCP server.
//!
//! The entrypoint parses configuration, checks the setup and hands the domain
//! to the shared host. Domain behavior lives in the modules.

use std::{net::SocketAddr, sync::LazyLock};

use clap::Parser;
use veoveo_mcp_contract::{
    GatewayInternalTrustBundle, PublicDeployment, hosting::Hosted, hosting::HostedServer,
    init_server_telemetry, parse_allowed_host_authority,
};

mod contract;
mod glossary;
mod server;
mod setup;
#[cfg(test)]
mod tests;

use server::GlossaryMcp;
use setup::SERVER_SETUP;

#[derive(Parser)]
#[command(name = "glossary-mcp", about = "Glossary MCP server (streamable HTTP)")]
struct Args {
    #[arg(long, default_value_t = 8810)]
    port: u16,
    #[arg(long, env = "PUBLIC_BASE_URL")]
    public_base_url: String,
    #[arg(long, default_value_t = false)]
    allow_loopback_hosts: bool,
    #[arg(long = "allowed-host", value_name = "HOST", value_parser = parse_allowed_host)]
    allowed_hosts: Vec<String>,
    /// Public Ed25519 JWKS used to verify gateway identity assertions.
    #[arg(long, env = "VEOVEO_INTERNAL_TRUST_JWKS", hide_env_values = true)]
    internal_trust_jwks: String,
}

fn parse_allowed_host(value: &str) -> Result<String, String> {
    parse_allowed_host_authority(value.trim())
        .map(|_| value.trim().to_owned())
        .ok_or_else(|| "expected a host authority such as glossary-mcp:8810".to_owned())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Check the declared setup before anything else starts.
    LazyLock::force(&SERVER_SETUP);
    let _telemetry = init_server_telemetry("veoveo-glossary-mcp", "info")?;
    let args = Args::parse();
    let server = HostedServer::for_domain::<GlossaryMcp>()
        .deployment(
            &PublicDeployment::new(&args.public_base_url)?,
            args.allow_loopback_hosts,
        )?
        .allowed_hosts(args.allowed_hosts)
        .internal_trust(GatewayInternalTrustBundle::from_json(
            &args.internal_trust_jwks,
        )?)?
        .handler(|| Hosted::new(GlossaryMcp::new()))
        .build();
    server
        .serve(SocketAddr::from(([0, 0, 0, 0], args.port)))
        .await
}
