use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "agent", about = "Veoveo agent kernel")]
pub(crate) struct Args {
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Cmd {
    /// Run an agent from a manifest until stopped.
    Run(RunArgs),
}

#[derive(Parser, Debug)]
pub(crate) struct RunArgs {
    /// Agent manifest JSON.
    #[arg(long)]
    pub(crate) manifest: PathBuf,
    /// Directory holding the agent's durable memory files.
    #[arg(long)]
    pub(crate) data_dir: PathBuf,
    /// Run one episode with this prompt at boot.
    #[arg(long)]
    pub(crate) prompt: Option<String>,
    /// Exit after the boot episode instead of watching detached tasks; the
    /// next boot resumes them from the ledger. Single-shot operation, and the
    /// deterministic way to exercise resume-across-processes.
    #[arg(long, default_value_t = false)]
    pub(crate) halt_after_episode: bool,
}
