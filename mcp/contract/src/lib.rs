//! Shared MCP server mechanics for provider-backed generation servers.
//!
//! The crate keeps provider-neutral concerns out of individual adapters:
//! task records, webhook waiters, resource subscriptions, URI conventions,
//! shared artifact and usage contract types, and the small provider trait that
//! normalizes catalog and prediction behavior.

/// Canonical Veoveo hosted MCP server contract revision.
pub const HOSTED_MCP_CONTRACT_REVISION: &str = "veoveo.ai/hosted-mcp/v4";

#[cfg(feature = "configuration")]
pub mod access;
#[cfg(feature = "analytics")]
#[cfg(feature = "runtime")]
pub mod analytics;
#[cfg(feature = "runtime")]
pub mod artifact_service;
#[cfg(feature = "runtime")]
pub mod bootstrap;
#[cfg(feature = "runtime")]
pub mod catalog;
#[cfg(feature = "runtime")]
pub mod deployment;
pub mod docs;
#[cfg(not(feature = "configuration"))]
pub use veoveo_types::ServerSlug;
#[cfg(feature = "configuration")]
pub mod gateway;
#[cfg(feature = "runtime")]
pub mod host;
#[cfg(feature = "runtime")]
pub mod hosting;
#[cfg(feature = "runtime")]
pub mod internal_auth;
#[cfg(feature = "runtime")]
pub mod pagination;
#[cfg(feature = "runtime")]
pub mod protocol;
#[cfg(feature = "runtime")]
pub mod provider;
#[cfg(feature = "runtime")]
pub mod server_contract;
#[cfg(feature = "runtime")]
pub mod subscriptions;
#[cfg(feature = "runtime")]
pub mod task_completion;
#[cfg(feature = "runtime")]
pub mod tasks;
#[cfg(feature = "runtime")]
pub mod telemetry;
#[cfg(feature = "runtime")]
pub mod transport;
#[cfg(feature = "runtime")]
pub mod uri;
#[cfg(feature = "runtime")]
pub mod usage;
#[cfg(feature = "runtime")]
pub mod waiters;
#[cfg(feature = "configuration")]
pub mod work_context;

#[cfg(feature = "configuration")]
pub use access::{
    AccessDecision, AccessRequest, GroupMembership, GroupRole, decide, grant_level_for_caller,
    mac_satisfied, role_in_group,
};
#[cfg(feature = "analytics")]
#[cfg(feature = "runtime")]
pub use analytics::{DuckDbAnalytics, SharedDuckDbConnection, open_duckdb};
#[cfg(feature = "runtime")]
pub use artifact_service::{
    ArtifactPlane, ArtifactPlaneError, ArtifactReadAuthority, ArtifactReadCapabilityScope,
    PlaneCaller,
};
#[cfg(feature = "runtime")]
pub use bootstrap::{
    SERVER_BOOTSTRAP_FLAG, SERVER_BOOTSTRAP_ISSUER, SERVER_BOOTSTRAP_MOUNT_PATH,
    SERVER_BOOTSTRAP_VALIDATE_COMMAND, ServerBootstrapDocument, ServerBootstrapError,
    server_bootstrap_principal,
};
#[cfg(feature = "runtime")]
pub use catalog::GatewayDiscoveryMetadata;
#[cfg(feature = "runtime")]
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
#[cfg(feature = "configuration")]
pub use gateway::{
    AccessTokenSubject, ArtifactAudience, AuthMethod, AuthMode, AuthOutcome, AuthReasonCode,
    AuthorizationServerEndpoint, CanonicalTaskId, CompatibilityHelperId, CompletionExposure,
    DataLabelDefinition, DiscoveryFailureMode, Exposure, GatewayAuthorizationCodeRecord,
    GatewayAuthorizationRequest, GatewayControlPlane, GatewayControlPlaneError,
    GatewayControlPlaneRevision, GatewayControlPlaneRevisionId, GatewayControlPlaneRevisionSource,
    GatewayJwtRevocation, GatewayJwtRevocationAdminStatus, GatewayJwtRevocationApplyResult,
    GatewayJwtRevocationPruneResult, GatewayJwtRevocationRequest, GatewayProfile, GatewayProfileId,
    GatewayRefreshFamilyId, GatewayRefreshGrant, GatewayRefreshRevocationRequest,
    GatewayResourceProjection, GatewayResourceSubscription, HttpsUrl, IdentityProvider,
    IdentityProviderClaimMapping, IdentityProviderEndpoint, IdentityProviderId,
    IdentityProviderOidcClientRegistration, IdentityProviderSubjectClaim,
    IdentityProviderTenantClaim, IdentityProviderTenantClaimMapping, JwksFilePath, JwksSource,
    JwtId, LocalToolName, MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION,
    MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION, McpMethodName, McpSurfaceCapabilities,
    McpSurfaceCapability, MountPath, OAuthAuthorizationCode, OAuthClientAuthMethod, OAuthClientId,
    OAuthClientRegistration, OAuthClientSurface, OAuthEndpointUrl, OAuthGrantType,
    OAuthRedirectUri, OAuthRefreshToken, OAuthStateValue, OAuthTokenTypeHint, OidcClientAuthMethod,
    OidcClientId, OidcClientRegistrationId, OidcNonce, OpaqueTaskId, OwnedRoute, OwnedRoutePurpose,
    PkceCodeChallenge, PkceCodeChallengeMethod, PkceCodeVerifier, PlatformCapabilityId,
    PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyRule, PolicyRuleId, PolicySet,
    PolicyTarget, Principal, PrincipalAssurance, PrincipalAuditAttributes, PrincipalDisplayName,
    PrincipalKind, ProfileServerExposure, PromptName, ResourceAuthorizationServer,
    ResourceProjectionMode, ResourceSelector, ResourceUriPrefix, ResourceUriTemplate,
    ServerManifest, ServerSlug, TaskExposure, TenantDefinition, TokenIssuer, TokenSubject, TraceId,
    UpstreamEndpoint, UpstreamTransport, composed_gateway_schema,
};
#[cfg(feature = "runtime")]
pub use host::{
    HostAuthority, host_authority_is_allowed, parse_allowed_host_authority,
    parse_request_host_authority, public_allowed_hosts,
};
#[cfg(feature = "runtime")]
pub use internal_auth::{
    DEFAULT_GATEWAY_INTERNAL_SIGNING_KEY_ID, GATEWAY_INTERNAL_TOKEN_ISSUER,
    GATEWAY_ROUTING_REQUIRED, GatewayInternalIdentity, GatewayInternalResourceIdentity,
    GatewayInternalResourceTokenVerifier, GatewayInternalSigningKey, GatewayInternalTokenIssuer,
    GatewayInternalTokenVerifier, GatewayInternalTrustBundle, GatewayRequestContext,
    GatewayRequestContextFormat, InternalTokenError, IssuedGatewayInternalResourceToken,
    IssuedGatewayInternalToken, VerifiedArtifactUploadIdentity,
};
#[cfg(feature = "runtime")]
pub use pagination::{Page, PaginationError, paginate};
#[cfg(feature = "runtime")]
pub use protocol::{
    MAX_BAGGAGE_BYTES, MAX_TRACESTATE_BYTES, PRIVATE_CATALOG_TTL_MS, PRIVATE_RESOURCE_TTL_MS,
    PUBLIC_IMMUTABLE_TTL_MS, final_protocol_versions, private_resource_response,
    sanitized_request_meta, trace_id_from_traceparent,
};
#[cfg(feature = "runtime")]
pub use provider::Provider;
#[cfg(feature = "runtime")]
pub use subscriptions::{
    ResourceListObservers, ResourceUpdate, SubscriptionHub, accepted_subscription_filter,
    accepted_task_subscription_filter, listen_resources, receive_resource_list_change,
    receive_resource_update, send_resource_update,
};
#[cfg(feature = "runtime")]
pub use tasks::{
    GATEWAY_TASK_RESOURCE_TEMPLATE, GatewayTaskStatus, GatewayTaskStatusDocument,
    GatewayTaskStatusKind, RELATED_TASK_META_KEY, gateway_task_resource_uri, notify_progress,
    now_utc, parse_gateway_task_resource_uri, related_task_meta, set_related_task_meta,
};
#[cfg(feature = "runtime")]
pub use telemetry::{TelemetryGuard, init_server_telemetry};
#[cfg(feature = "runtime")]
pub use transport::{
    MAX_MCP_RESPONSE_BYTES, bound_serialized_jsonrpc_response,
    canonical_streamable_http_server_config, enforce_serialized_mcp_response,
    stateless_session_manager,
};
#[cfg(feature = "runtime")]
pub use uri::{
    ServerResourceUri, ServerResourceUriError, ServerResourceUris, parse_server_doc_uri,
};
#[cfg(feature = "runtime")]
pub use usage::{UsageKind, UsageRecord, UsageReport};
#[cfg(feature = "runtime")]
pub use waiters::WebhookWaiters;
#[cfg(feature = "configuration")]
pub use work_context::{WorkContextDefinition, WorkContextMembershipRule};

/// Unified platform audit contract, also used by non-MCP producers.
#[cfg(feature = "runtime")]
pub mod audit {
    pub use veoveo_audit_contract::*;
}

#[cfg(all(test, feature = "configuration"))]
#[path = "../../../testing/fixtures/catalog_registry.rs"]
mod catalog_fixture;

#[cfg(feature = "runtime")]
pub mod fixture_client;
