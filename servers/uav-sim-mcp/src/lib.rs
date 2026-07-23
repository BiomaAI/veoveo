//! Governed UAV simulation MCP domain.
//!
//! The public contract is simulator-neutral. The reference adapter connects it
//! to Isaac Sim, Cesium for Omniverse, Pegasus, and PX4 over a private typed
//! HTTP boundary.

pub mod adapter;
pub mod contract;
mod live_app;
pub mod uris;

pub mod server;
