//! Private protocol used between the Rust MCP control plane and cuOpt workers.

mod client;
pub mod protocol;

pub use client::{ExecutorClient, ExecutorClientError};
pub use protocol::*;
