//! The agent process: one manifest, one data dir, one long life.

use anyhow::Result;
use clap::Parser;
use veoveo_mcp_contract::{TelemetryGuard, init_server_telemetry};

#[path = "agent/cli.rs"]
mod cli;
#[path = "agent/run.rs"]
mod run;

use cli::{Args, Cmd};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-agent-kernel", "info,veoveo_agent_kernel=debug")?;
    let args = Args::parse();
    match args.cmd {
        Cmd::Run(run_args) => run::cmd_run(run_args).await,
    }
}
