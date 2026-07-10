//! Final MCP task extension transport for Veoveo MCP servers.
//!
//! The durable runtime remains protocol-neutral. This crate owns extension
//! negotiation, JSON-RPC wire models, routing headers, and subscription SSE.
//! Veoveo agents use the typed `ai.bioma.veoveo/taskRetentionPin` request-meta
//! field so task creation and result-retention protection commit atomically.

mod adapter;
mod models;
mod projection;

pub use adapter::{
    AdapterError, ServerDiscovery, TaskExtensionAdapter, TaskExtensionHandler, TaskSubscription,
    task_extension_middleware,
};
pub use models::*;
pub use projection::{project_snapshot, task_seed};
