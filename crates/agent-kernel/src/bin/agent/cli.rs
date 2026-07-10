use std::path::PathBuf;

use clap::{Parser, Subcommand};
use secrecy::SecretString;

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
    /// Query the agent's episodic decision log (RRD segments) as JSON rows.
    Timeline(TimelineArgs),
    /// Rebuild domain tables from the decision log into memory.replayed.duckdb.
    Replay(ReplayArgs),
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
    /// next boot resumes them from the memory. Single-shot operation, and the
    /// deterministic way to exercise resume-across-processes.
    #[arg(long, default_value_t = false)]
    pub(crate) halt_after_episode: bool,
    /// Tee the decision log to a live Rerun viewer, e.g.
    /// rerun+http://127.0.0.1:9876/proxy.
    #[arg(long)]
    pub(crate) viewer_tee: Option<String>,
    #[arg(long, env = "VEOVEO_SURREAL_ENDPOINT")]
    pub(crate) surreal_endpoint: String,
    #[arg(long, env = "VEOVEO_SURREAL_NAMESPACE")]
    pub(crate) surreal_namespace: String,
    #[arg(long, env = "VEOVEO_SURREAL_DATABASE")]
    pub(crate) surreal_database: String,
    #[arg(long, env = "VEOVEO_SURREAL_AUTH_LEVEL")]
    pub(crate) surreal_auth_level: String,
    #[arg(long, env = "VEOVEO_SURREAL_USERNAME")]
    pub(crate) surreal_username: String,
    #[arg(
        long,
        env = "VEOVEO_SURREAL_PASSWORD",
        hide_env_values = true,
        value_parser = parse_secret
    )]
    pub(crate) surreal_password: SecretString,
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

fn parse_secret(value: &str) -> Result<SecretString, String> {
    (!value.is_empty())
        .then(|| SecretString::from(value))
        .ok_or_else(|| "secret must not be empty".to_owned())
}
