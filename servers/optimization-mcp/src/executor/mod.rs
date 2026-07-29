//! Private protocol used between the Rust MCP control plane and cuOpt workers.

mod client;
pub mod protocol;

pub use client::{DEFAULT_MAX_EXECUTOR_FRAME_BYTES, ExecutorClient, ExecutorClientError};
pub use protocol::*;
