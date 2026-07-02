//! Shared MCP server mechanics for provider-backed generation servers.
//!
//! The crate keeps provider-neutral concerns out of individual adapters:
//! task records, webhook/poll waiters, resource subscriptions, URI conventions,
//! and the small provider trait that normalizes catalog and prediction behavior.

pub mod provider;
pub mod subscriptions;
pub mod tasks;
pub mod uri;
pub mod waiters;

pub use provider::Provider;
pub use subscriptions::SubscriptionHub;
pub use tasks::{TaskPayloadState, TaskStore, notify_progress, notify_task_status, now_iso};
pub use uri::ProviderUris;
pub use waiters::WebhookWaiters;
