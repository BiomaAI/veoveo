//! SUMO traffic-world MCP domain.
//!
//! The binary owns exactly one serialized TraCI connection, publishes typed
//! world frames to the Recording Hub, and uses Veoveo's shared durable task
//! runtime for long operations.

#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod driver;
#[cfg(feature = "runtime")]
pub mod recording;
#[cfg(feature = "mcp")]
pub mod server;
