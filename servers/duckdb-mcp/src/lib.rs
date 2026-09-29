//! Hosted DuckDB MCP server library.
//!
//! Owner-scoped mutable database files, a hardened in-process DuckDB engine
//! (no external access from SQL), immutable artifact exports, and durable
//! task and usage state in the shared platform store.

#[cfg(feature = "runtime")]
pub mod artifacts;
#[cfg(feature = "runtime")]
pub mod catalog;
#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "contract")]
pub use contract::*;
#[cfg(feature = "runtime")]
pub mod engine;
#[cfg(feature = "contract")]
pub mod uris;

#[cfg(feature = "runtime")]
pub mod usage;
