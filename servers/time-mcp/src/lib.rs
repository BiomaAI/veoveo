//! Authoritative temporal reasoning for Veoveo agents.

pub mod authority;
pub mod clock;
pub mod contract;
pub mod engine;

pub use contract::*;

pub async fn run() -> anyhow::Result<()> {
    anyhow::bail!("time-mcp hosting is added in the server milestone")
}
