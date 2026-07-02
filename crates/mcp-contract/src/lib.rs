//! Shared MCP server mechanics for provider-backed generation servers.
//!
//! The crate keeps provider-neutral concerns out of individual adapters:
//! task records, webhook waiters, resource subscriptions, URI conventions,
//! shared artifact and usage contract types, and the small provider trait that
//! normalizes catalog and prediction behavior.

pub mod analytics;
pub mod deployment;
pub mod gateway;
pub mod generation;
pub mod pagination;
pub mod provider;
pub mod storage;
pub mod subscriptions;
pub mod tasks;
pub mod uri;
pub mod usage;
pub mod waiters;

pub use analytics::{DuckDbAnalytics, SharedDuckDbConnection, open_duckdb};
pub use deployment::{PublicDeployment, ServerPublicEndpoint};
pub use gateway::{
    AccessTokenSubject, AuditEvent, AuthMode, CompletionExposure, DataLabelId, Exposure,
    GatewayAction, GatewayControlPlane, GatewayControlPlaneError, GatewayProfile, GatewayProfileId,
    GatewayTaskId, GatewayToolName, GroupId, HttpsUrl, IdentifierError, IdentityProvider,
    IdentityProviderId, JwtId, LocalToolName, McpMethodName, McpSurfaceCapabilities, MountPath,
    OwnedRoute, OwnedRoutePurpose, PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyRule,
    PolicyRuleId, PolicySet, PolicyTarget, PolicyVersion, Principal, PrincipalId, PrincipalKind,
    ProfileServerExposure, ProtectedResourceId, ResourceScheme, ResourceSelector, ResourceUri,
    ResourceUriPrefix, ResourceUriTemplate, RoleId, ScopeName, SecretLocator, SecretOwner,
    SecretPurpose, SecretReference, SecretReferenceId, SecretSource, ServerManifest, ServerSlug,
    TaskExposure, TenantId, TokenIssuer, TokenSubject, TraceId, UpstreamEndpoint, UpstreamTaskId,
    UpstreamTransport,
};
pub use generation::{GenerationPredictionSummary, GenerationRunOutput};
pub use pagination::{Page, PaginationError, paginate};
pub use provider::Provider;
pub use storage::{ArtifactMetadata, ArtifactObject, ArtifactPut, ComplianceMetadata};
pub use subscriptions::SubscriptionHub;
pub use tasks::{
    TaskPayloadState, TaskStore, notify_progress, notify_task_status, now_iso, now_utc,
};
pub use uri::{ProviderUris, artifact_object_key, is_sha256};
pub use usage::{UsageKind, UsageRecord, UsageReport};
pub use waiters::WebhookWaiters;
