//! Shared MCP server mechanics for provider-backed generation servers.
//!
//! The crate keeps provider-neutral concerns out of individual adapters:
//! task records, webhook waiters, resource subscriptions, URI conventions,
//! shared artifact and usage contract types, and the small provider trait that
//! normalizes catalog and prediction behavior.

#[cfg(feature = "analytics")]
pub mod analytics;
pub mod deployment;
pub mod gateway;
pub mod generation;
pub mod internal_auth;
pub mod pagination;
pub mod provider;
pub mod storage;
pub mod subscriptions;
pub mod tasks;
pub mod telemetry;
pub mod uri;
pub mod usage;
pub mod waiters;

#[cfg(feature = "analytics")]
pub use analytics::{DuckDbAnalytics, SharedDuckDbConnection, open_duckdb};
pub use deployment::{
    DataRetentionPolicy, DeploymentEndpoint, DeploymentProfileId, DeploymentProfileKind,
    DeploymentRequirementId, DeploymentServiceKind, GatewayToServerIdentity, NetworkBoundaryRule,
    NetworkTarget, ObjectStoreDeployment, ObjectStoreKind, PublicDeployment, RegulatedDataControls,
    SelfHostedDeploymentPlan, SelfHostedDeploymentProfile, ServerPublicEndpoint,
    ServiceToServiceSecurity, ServiceToServiceTransport, TelemetrySignal, TelemetrySinkDeployment,
    TelemetrySinkKind,
};
pub use gateway::{
    AccessTokenSubject, AuditEvent, AuthAuditEvent, AuthMethod, AuthMode, AuthOutcome,
    AuthReasonCode, AuthorizationServerEndpoint, AuthorizationServerId,
    CertificateAuthorityFilePath, CertificateAuthoritySource, CompletionExposure, DataLabelId,
    Exposure, GatewayAction, GatewayAuthorizationCodeRecord, GatewayAuthorizationRequest,
    GatewayControlPlane, GatewayControlPlaneError, GatewayControlPlaneRevision,
    GatewayControlPlaneRevisionId, GatewayControlPlaneRevisionSource, GatewayJwtRevocation,
    GatewayJwtRevocationAdminStatus, GatewayJwtRevocationApplyResult,
    GatewayJwtRevocationPruneResult, GatewayJwtRevocationRequest, GatewayProfile, GatewayProfileId,
    GatewayResourceProjection, GatewayResourceSubscription, GatewayTaskId, GatewayTaskMapping,
    GatewayToolName, GroupId, HttpsUrl, IdentifierError, IdentityProvider,
    IdentityProviderEndpoint, IdentityProviderId, IdentityProviderOidcClientRegistration,
    JwksFilePath, JwksSource, JwtId, LocalToolName, MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION,
    MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION, McpMethodName, McpSurfaceCapabilities,
    McpSurfaceCapability, MountPath, OAuthAuthorizationCode, OAuthClientAuthMethod, OAuthClientId,
    OAuthClientRegistration, OAuthGrantType, OAuthRedirectUri, OAuthStateValue,
    OidcClientAuthMethod, OidcClientId, OidcClientRegistrationId, OidcNonce, OwnedRoute,
    OwnedRoutePurpose, PkceCodeChallenge, PkceCodeChallengeMethod, PkceCodeVerifier,
    PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyRule, PolicyRuleId, PolicySet,
    PolicyTarget, PolicyVersion, Principal, PrincipalAssurance, PrincipalId, PrincipalKind,
    ProfileServerExposure, PromptName, ProtectedResourceId, ResourceAuthorizationServer,
    ResourceScheme, ResourceSelector, ResourceUri, ResourceUriPrefix, ResourceUriTemplate, RoleId,
    ScopeName, SecretLocator, SecretOwner, SecretPurpose, SecretReference, SecretReferenceId,
    SecretSource, ServerManifest, ServerSlug, TaskExposure, TaskIdProjection, TenantId,
    TokenIssuer, TokenSubject, TraceId, UpstreamEndpoint, UpstreamTaskId, UpstreamTransport,
    UpstreamTransportSecurity, UpstreamUrl,
};
pub use generation::{GenerationPredictionSummary, GenerationRunOutput};
pub use internal_auth::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalIdentity, GatewayInternalTokenIssuer,
    GatewayInternalTokenVerifier, InternalTokenError, InternalTokenSecret,
    IssuedGatewayInternalToken, MIN_INTERNAL_TOKEN_SECRET_BYTES,
};
pub use pagination::{Page, PaginationError, paginate};
pub use provider::Provider;
pub use storage::{ArtifactMetadata, ArtifactObject, ArtifactPut, ComplianceMetadata};
pub use subscriptions::SubscriptionHub;
pub use tasks::{
    PrunedTask, TaskPayloadState, TaskStore, notify_progress, notify_task_status, now_iso, now_utc,
};
pub use telemetry::{TelemetryGuard, init_server_telemetry};
pub use uri::{
    ServerResourceUri, ServerResourceUriError, ServerResourceUris, artifact_object_key, is_sha256,
};
pub use usage::{UsageKind, UsageRecord, UsageReport};
pub use waiters::WebhookWaiters;
