//! MCP Apps (SEP-1865 / ext-apps) support: servers ship interactive HTML
//! views as `ui://` resources; hosts render them in sandboxed iframes and
//! bridge `tools/call` over postMessage. This crate owns the pinned protocol
//! constants plus the server- and host-side helpers. Unlike the tasks
//! extension there are no new MCP-layer JSON-RPC methods — everything custom
//! happens between host and view — so no transport adapter is needed.

mod client;
mod models;
mod server;

pub use client::*;
pub use models::*;
pub use server::*;
