//! Authoritative temporal reasoning for Veoveo agents.

pub mod authority;
pub mod catalog;
pub mod clock;
pub mod contract;
pub mod engine;
pub mod registry;

pub use contract::*;

mod acquisition;
mod admin;
pub mod mcp;
pub mod prompts;
mod server;
pub mod state;
pub mod uris;

pub async fn run() -> anyhow::Result<()> {
    server::run().await
}
