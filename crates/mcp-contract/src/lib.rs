//! Shared MCP server mechanics for provider-backed generation servers.
//!
//! The crate keeps provider-neutral concerns out of individual adapters:
//! task records, webhook waiters, resource subscriptions, URI conventions,
//! shared artifact and usage contract types, and the small provider trait that
//! normalizes catalog and prediction behavior.

pub mod analytics;
pub mod deployment;
pub mod provider;
pub mod storage;
pub mod subscriptions;
pub mod tasks;
pub mod uri;
pub mod usage;
pub mod waiters;

pub use analytics::{DuckDbAnalytics, SharedDuckDbConnection, open_duckdb};
pub use deployment::{PublicDeployment, ServerPublicEndpoint};
pub use provider::Provider;
pub use storage::{ArtifactMetadata, ArtifactObject, ArtifactPut, ComplianceMetadata};
pub use subscriptions::SubscriptionHub;
pub use tasks::{
    TaskPayloadState, TaskStore, notify_progress, notify_task_status, now_iso, now_utc,
};
pub use uri::{ProviderUris, artifact_object_key, is_sha256};
pub use usage::{UsageKind, UsageRecord, UsageReport};
pub use waiters::WebhookWaiters;
