//! Durable scheduling and delivery state for autonomous Veoveo agents.
//!
//! The platform store is authoritative. In-process notifications and Surreal
//! LIVE streams are latency hints only; every recovery path starts from the
//! persisted pending rows.

#[cfg(feature = "runtime")]
mod control;
#[cfg(feature = "runtime")]
mod runtime;
#[cfg(feature = "runtime")]
mod types;

#[cfg(feature = "runtime")]
pub use control::*;
#[cfg(feature = "runtime")]
pub use runtime::AgentRuntime;
#[cfg(feature = "runtime")]
pub use types::*;

#[cfg(feature = "schema")]
pub mod schema;

#[cfg(feature = "gateway")]
pub mod gateway;
