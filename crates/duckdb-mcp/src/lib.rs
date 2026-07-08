//! Hosted DuckDB MCP server library.
//!
//! Owner-scoped mutable database files, a hardened in-process DuckDB engine
//! (no external access from SQL), immutable artifact exports, and durable
//! ownership/usage state.

pub mod artifacts;
pub mod contract;
pub mod engine;
pub mod state;
pub mod uris;
