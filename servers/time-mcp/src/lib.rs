//! Authoritative temporal reasoning for Veoveo agents.

#[cfg(feature = "runtime")]
pub mod authority;
#[cfg(feature = "runtime")]
pub mod catalog;
#[cfg(feature = "runtime")]
pub mod clock;
#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod engine;
#[cfg(feature = "runtime")]
pub mod registry;

#[cfg(feature = "contract")]
pub use contract::*;

#[cfg(feature = "runtime")]
pub mod acquisition;
#[cfg(feature = "mcp")]
mod admin;
#[cfg(feature = "runtime")]
mod index;
#[cfg(feature = "mcp")]
pub mod mcp;
#[cfg(feature = "mcp")]
pub mod prompts;
#[cfg(feature = "mcp")]
mod server;
#[cfg(feature = "runtime")]
pub mod state;
#[cfg(feature = "contract")]
pub mod uris;

#[cfg(feature = "mcp")]
pub async fn run() -> anyhow::Result<()> {
    server::run().await
}

#[cfg(feature = "runtime")]
mod persistence;
#[cfg(all(test, feature = "runtime"))]
#[path = "../../../testing/fixtures/store.rs"]
mod test_store;
