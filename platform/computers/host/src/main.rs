mod config;
mod files;
mod process;
mod runtime;

use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Run {
        #[arg(long)]
        config: PathBuf,
    },
    Health,
}
#[tokio::main]
async fn main() -> Result<()> {
    match Args::parse().command {
        Action::Health => runtime::health().await,
        Action::Run { config } => {
            ensure!(
                nix::unistd::geteuid().is_root(),
                "compute host requires its privileged container"
            );
            let config = std::fs::canonicalize(config)?;
            let config: config::Config = serde_json::from_slice(&files::read(&config)?)?;
            config.validate()?;
            runtime::run(config).await
        }
    }
}
