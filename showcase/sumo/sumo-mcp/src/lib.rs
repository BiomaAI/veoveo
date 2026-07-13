//! SUMO traffic-world MCP domain.
//!
//! The binary owns exactly one serialized TraCI connection, publishes typed
//! world frames to the Recording Hub, and uses Veoveo's shared durable task
//! runtime for long operations.

pub mod contract;
pub mod driver;
pub mod recording;
pub mod server;
