//! Governed UAV simulation MCP domain.
//!
//! The public contract is simulator-neutral. The reference adapter connects it
//! to Isaac Sim, Cesium for Omniverse, Newton, Warp, and PX4 over a private typed
//! HTTP boundary.

#[cfg(feature = "runtime")]
pub mod adapter;
#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod uris;
#[cfg(feature = "runtime")]
pub mod world;

#[cfg(feature = "mcp")]
mod live_app;
#[cfg(feature = "mcp")]
pub mod server;
