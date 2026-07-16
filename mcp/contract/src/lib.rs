//! Shared MCP server mechanics for provider-backed generation servers.
//!
//! The crate keeps provider-neutral concerns out of individual adapters:
//! task records, webhook waiters, resource subscriptions, URI conventions,
//! shared artifact and usage contract types, and the small provider trait that
//! normalizes catalog and prediction behavior.

pub mod access;
#[cfg(feature = "analytics")]
pub mod analytics;
pub mod artifact_service;
pub mod coordinates;
pub mod deployment;
pub mod duckdb;
pub mod gateway;
pub mod generation;
pub mod host;
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

pub use access::{
    ARTIFACT_PLANE_SCHEME, AccessDecision, AccessLevel, AccessRequest, ArtifactId, ArtifactIdError,
    Grant, GroupMembership, GroupRole, Subject, decide, grant_level_for_caller, mac_satisfied,
    parse_artifact_plane_uri, role_in_group,
};
#[cfg(feature = "analytics")]
pub use analytics::{DuckDbAnalytics, SharedDuckDbConnection, open_duckdb};
pub use artifact_service::{
    ArtifactPage, ArtifactPlane, ArtifactPlaneError, ArtifactShareLink, ArtifactShareLinkId,
    ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret, ArtifactWriteIdempotencyKey,
    CreateArtifactShareLinkRequest, GrantList, IssueArtifactWriteCapabilityRequest,
    IssuedArtifactWriteCapability, ListArtifactsRequest, PlaneCaller, PutArtifactRequest,
    PutGrantRequest, RedeemArtifactWriteCapabilityRequest, SetArtifactReleaseStateRequest,
};
pub use coordinates::{
    CoordinateIdError, CoordinateOperationId, CoordinateOperationKind,
    CoordinateOperationProvenance, CoordinateOperationRef, CrsId, DatumId, EllipsoidId, FrameId,
    FrameKind, GeofenceId, GeofenceRule, GeofenceViolation, TrajectoryId,
};
pub use deployment::{
    AnalyticalRuntimeDeployment, AnalyticalRuntimeEngine, AnalyticalRuntimePurpose,
    ChangefeedSourceOfTruth, ConnectivityMode, DataRetentionPolicy, DatabaseHighAvailability,
    DatabaseTopology, DeploymentEndpoint, DeploymentProfileId, DeploymentRequirementId,
    DeploymentServiceKind, ExternalDataAccess, GatewayToServerIdentity, IdentityProviderDeployment,
    IdentityProviderKind, IngressDeployment, IngressKind, InstallationScope, LiveQueryRole,
    ObjectStoreDeployment, ObjectStoreKind, PlatformStoreDeployment, PlatformStoreEngine,
    PublicDeployment, SecretManagerDeployment, SecretManagerKind, SelfHostedDeploymentPlan,
    SelfHostedDeploymentProfile, ServerPublicEndpoint, ServiceToServiceSecurity,
    ServiceToServiceTransport, SurrealDbVersion, SurrealStorageEngine, TelemetryCollectorKind,
    TelemetryDeployment, TelemetrySignal, TenantModel, TenantModelKind,
};
pub use duckdb::{
    DuckDbFormat, DuckDbReadOptions, DuckDbSource, DuckDbSqlBuildError, duckdb_quote_identifier,
    duckdb_quote_literal, duckdb_read_function_sql, duckdb_read_options_sql,
};
pub use gateway::{
    AccessTokenSubject, AuditEvent, AuthAuditEvent, AuthMethod, AuthMode, AuthOutcome,
    AuthReasonCode, AuthorizationServerEndpoint, AuthorizationServerId, CanonicalTaskId,
    CertificateAuthorityFilePath, CertificateAuthoritySource, CompatibilityHelperId,
    CompletionExposure, DataLabelDefinition, DataLabelId, Exposure, GatewayAction,
    GatewayAuthorizationCodeRecord, GatewayAuthorizationRequest, GatewayControlPlane,
    GatewayControlPlaneError, GatewayControlPlaneRevision, GatewayControlPlaneRevisionId,
    GatewayControlPlaneRevisionSource, GatewayJwtRevocation, GatewayJwtRevocationAdminStatus,
    GatewayJwtRevocationApplyResult, GatewayJwtRevocationPruneResult, GatewayJwtRevocationRequest,
    GatewayProfile, GatewayProfileId, GatewayRefreshFamilyId, GatewayRefreshGrant,
    GatewayRefreshRevocationRequest, GatewayResourceProjection, GatewayResourceSubscription,
    GatewayToolName, GroupId, HttpsUrl, IdentifierError, IdentityProvider,
    IdentityProviderClaimMapping, IdentityProviderEndpoint, IdentityProviderId,
    IdentityProviderOidcClientRegistration, IdentityProviderSubjectClaim,
    IdentityProviderTenantClaim, IdentityProviderTenantClaimMapping, JwksFilePath, JwksSource,
    JwtId, LocalToolName, MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION,
    MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION, McpMethodName, McpSurfaceCapabilities,
    McpSurfaceCapability, MountPath, OAuthAuthorizationCode, OAuthClientAuthMethod, OAuthClientId,
    OAuthClientRegistration, OAuthClientSurface, OAuthEndpointUrl, OAuthGrantType,
    OAuthRedirectUri, OAuthRefreshToken, OAuthStateValue, OAuthTokenTypeHint, OidcClientAuthMethod,
    OidcClientId, OidcClientRegistrationId, OidcNonce, OwnedRoute, OwnedRoutePurpose,
    PkceCodeChallenge, PkceCodeChallengeMethod, PkceCodeVerifier, PolicyDecision, PolicyEffect,
    PolicyReasonCode, PolicyRule, PolicyRuleId, PolicySet, PolicyTarget, PolicyVersion, Principal,
    PrincipalAssurance, PrincipalAuditAttributes, PrincipalId, PrincipalKind,
    ProfileServerExposure, PromptName, ProtectedResourceId, ProtectedResourceName, ProviderTaskId,
    RecordingApplicationId, RecordingDatasetName, RecordingIngestResource, RecordingProducerId,
    RecordingProducerQuotas, RecordingProducerRegistration, RecordingRetentionPolicy,
    ResourceAuthorizationServer, ResourceProjectionMode, ResourceScheme, ResourceSelector,
    ResourceUri, ResourceUriPrefix, ResourceUriTemplate, RoleId, ScopeName, SecretLocator,
    SecretOwner, SecretPurpose, SecretReference, SecretReferenceId, SecretSource, ServerManifest,
    ServerSlug, TaskExposure, TenantDefinition, TenantId, TokenIssuer, TokenSubject, TraceId,
    UpstreamEndpoint, UpstreamTransport, UpstreamTransportSecurity, UpstreamUrl,
};
pub use generation::{GenerationPredictionSummary, GenerationRunOutput};
pub use host::{
    HostAuthority, host_authority_is_allowed, parse_allowed_host_authority,
    parse_request_host_authority, public_allowed_hosts,
};
pub use internal_auth::{
    DEFAULT_GATEWAY_INTERNAL_SIGNING_KEY_ID, GATEWAY_INTERNAL_TOKEN_ISSUER,
    GatewayInternalIdentity, GatewayInternalSigningKey, GatewayInternalTokenIssuer,
    GatewayInternalTokenVerifier, GatewayInternalTrustBundle, InternalTokenError,
    IssuedGatewayInternalToken,
};
pub use pagination::{Page, PaginationError, paginate};
pub use provider::Provider;
pub use storage::{
    ArtifactMetadata, ArtifactObject, ArtifactPut, ArtifactReleaseState, ComplianceMetadata,
};
pub use subscriptions::SubscriptionHub;
pub use tasks::{
    GATEWAY_TASK_RESOURCE_TEMPLATE, GatewayTaskStatus, GatewayTaskStatusDocument,
    GatewayTaskStatusKind, RELATED_TASK_META_KEY, gateway_task_resource_uri, notify_progress,
    now_utc, parse_gateway_task_resource_uri, related_task_meta, set_related_task_meta,
};
pub use telemetry::{TelemetryGuard, init_server_telemetry};
pub use uri::{ServerResourceUri, ServerResourceUriError, ServerResourceUris};
pub use usage::{UsageKind, UsageRecord, UsageReport};
pub use waiters::WebhookWaiters;
