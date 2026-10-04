//! Governed UAV simulation MCP domain.
//!
//! The public contract is simulator-neutral. The reference adapter connects it
//! to Isaac Sim, Cesium for Omniverse, Newton, Warp, and PX4 over a private typed
//! HTTP boundary.

#[cfg(feature = "runtime")]
pub mod adapter;
#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "contract")]
pub mod uris;

#[cfg(feature = "mcp")]
mod live_app;
#[cfg(feature = "mcp")]
pub mod server;

#[cfg(feature = "schema")]
pub mod schema;

#[cfg(feature = "schema")]
pub mod observation;
#[cfg(feature = "schema")]
pub use observation::UavObservationTable;
