//! Veoveo Map MCP domain library.
//!
//! The crate owns Earth-referenced geography, governed transport data,
//! mobility profiles, routing, and dataset administration.

#[cfg(feature = "runtime")]
pub mod acquisition;
#[cfg(feature = "mcp")]
mod admin;
#[cfg(feature = "runtime")]
pub mod administration;
#[cfg(feature = "runtime")]
pub mod analytics;
#[cfg(feature = "runtime")]
pub mod artifacts;
#[cfg(feature = "runtime")]
pub mod authoring;
#[cfg(feature = "runtime")]
pub mod catalog;
#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod derivations;
#[cfg(feature = "runtime")]
pub mod feature_packages;
#[cfg(feature = "runtime")]
pub mod geodesy;
#[cfg(feature = "runtime")]
pub mod geography;
#[cfg(feature = "runtime")]
pub mod knowledge;
#[cfg(feature = "mcp")]
pub mod mcp;
#[cfg(feature = "mcp")]
pub mod prompts;
#[cfg(feature = "runtime")]
pub mod raster;
#[cfg(feature = "runtime")]
pub mod release_products;
#[cfg(any(feature = "mcp", all(test, feature = "runtime")))]
mod resource_changes;
#[cfg(feature = "runtime")]
pub mod routes;
#[cfg(feature = "mcp")]
mod server;
#[cfg(feature = "runtime")]
pub mod spatial;
#[cfg(feature = "runtime")]
pub mod state;
#[cfg(feature = "runtime")]
pub mod travel_models;
#[cfg(feature = "contract")]
pub mod uris;

#[cfg(feature = "mcp")]
pub async fn run() -> anyhow::Result<()> {
    server::run().await
}

#[cfg(feature = "contract")]
pub use contract::*;

#[cfg(all(test, feature = "runtime"))]
#[path = "../../../testing/fixtures/store.rs"]
mod test_store;
