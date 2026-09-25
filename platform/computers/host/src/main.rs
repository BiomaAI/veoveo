mod config;
mod files;
mod isolation;
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
    Init {
        #[arg(long)]
        config: PathBuf,
    },
    Run {
        #[arg(long)]
        config: PathBuf,
    },
    Health,
}
fn main() -> Result<()> {
    let action = Args::parse().command;
    if let Action::Init { config } = action {
        return isolation::initialize(&config);
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(action))
}
async fn run(action: Action) -> Result<()> {
    match action {
        Action::Init { .. } => unreachable!("initialization precedes runtime threads"),
        Action::Health => runtime::health().await,
        Action::Run { config } => {
            isolation::verify_runtime()?;
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
