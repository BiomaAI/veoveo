//! Durable scheduling and delivery state for autonomous Veoveo agents.
//!
//! The platform store is authoritative. In-process notifications and Surreal
//! LIVE streams are latency hints only; every recovery path starts from the
//! persisted pending rows.

mod runtime;
mod types;

pub use runtime::AgentRuntime;
pub use types::*;
