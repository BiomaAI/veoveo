//! Governed UAV simulation MCP domain.
//!
//! The public contract is simulator-neutral. The reference adapter connects it
//! to Isaac Sim, Cesium for Omniverse, Pegasus, and PX4 over a private typed
//! HTTP boundary.

pub mod adapter;
pub mod contract;
pub mod uris;
pub mod world;

mod view_scene;

pub mod server;
