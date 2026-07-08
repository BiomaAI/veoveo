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
    /// Query the agent's episodic flight log (RRD segments) as JSON rows.
    Timeline(TimelineArgs),
    /// Rebuild domain tables from the flight log into memory.replayed.duckdb.
    Replay(ReplayArgs),
    /// Wake a running agent with an operator message.
    Ask(AskArgs),
    /// Show a running agent's status.
    Status(StatusArgs),
}

#[derive(Parser, Debug)]
pub(crate) struct TimelineArgs {
    /// Directory holding the agent's durable memory files.
    #[arg(long)]
    pub(crate) data_dir: PathBuf,
    /// RRD segment directory name inside the data dir.
    #[arg(long, default_value = "rrd")]
    pub(crate) rrd_dir: String,
    /// Entity path filter (Rerun syntax), e.g. /agent/**.
    #[arg(long, default_value = "/**")]
    pub(crate) entities: String,
    /// Index timeline: log_time or episode.
    #[arg(long, default_value = "log_time")]
    pub(crate) timeline: String,
    #[arg(long, default_value_t = 50)]
    pub(crate) max_rows: u64,
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
    /// Tee the flight log to a live Rerun viewer, e.g.
    /// rerun+http://127.0.0.1:9876/proxy.
    #[arg(long)]
    pub(crate) viewer_tee: Option<String>,
    /// Fixed operator endpoint port (default: ephemeral, written to
    /// {data-dir}/operator.port).
    #[arg(long)]
    pub(crate) operator_port: Option<u16>,
}

#[derive(Parser, Debug)]
pub(crate) struct ReplayArgs {
    /// Agent manifest JSON.
    #[arg(long)]
    pub(crate) manifest: PathBuf,
    /// Directory holding the agent's durable memory files.
    #[arg(long)]
    pub(crate) data_dir: PathBuf,
}

#[derive(Parser, Debug)]
pub(crate) struct AskArgs {
    /// Directory holding the agent's durable memory files.
    #[arg(long)]
    pub(crate) data_dir: PathBuf,
    /// The message to wake the agent with.
    pub(crate) text: String,
}

#[derive(Parser, Debug)]
pub(crate) struct StatusArgs {
    /// Directory holding the agent's durable memory files.
    #[arg(long)]
    pub(crate) data_dir: PathBuf,
}
