//! Veoveo Map MCP domain library.
//!
//! The crate owns Earth-referenced geography, governed transport data,
//! mobility profiles, routing, and dataset administration.

pub mod acquisition;
pub mod administration;
pub mod analytics;
pub mod artifacts;
pub mod catalog;
pub mod contract;
pub mod geodesy;
pub mod geography;
pub mod mcp;
pub mod prompts;
pub mod release_products;
pub mod routes;
mod server;
pub mod state;
pub mod uris;

pub async fn run() -> anyhow::Result<()> {
    server::run().await
}

pub use contract::*;
