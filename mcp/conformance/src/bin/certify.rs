use std::path::PathBuf;

use anyhow::{Result, ensure};
use clap::Parser;
use veoveo_mcp_conformance::{
    ConformanceCredentials, HostedServerConformanceProfile, run_hosted_server_conformance,
};

#[derive(Debug, Parser)]
#[command(
    name = "veoveo-mcp-certify",
    about = "Certify one running Veoveo hosted MCP server"
)]
struct Args {
    /// Typed domain-neutral hosted-server profile.
    #[arg(long)]
    profile: PathBuf,
    /// Machine-readable conformance report.
    #[arg(long, default_value = "conformance-report.json")]
    report: PathBuf,
    /// Bearer presented to the server under test.
    #[arg(long, env = "MCP_BEARER_TOKEN", hide_env_values = true)]
    bearer_token: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let profile: HostedServerConformanceProfile =
        serde_json::from_slice(&std::fs::read(&args.profile)?)?;
    let credentials = args
        .bearer_token
        .map(ConformanceCredentials::bearer)
        .unwrap_or_default();
    let report = run_hosted_server_conformance(&profile, &credentials).await?;
    if let Some(parent) = args.report.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(&report)?;
    bytes.push(b'\n');
    std::fs::write(&args.report, bytes)?;
    println!(
        "conformance {}: {} check(s), report {}",
        if report.passed() { "passed" } else { "failed" },
        report.checks.len(),
        args.report.display()
    );
    ensure!(report.passed(), "hosted-server conformance failed");
    Ok(())
}
