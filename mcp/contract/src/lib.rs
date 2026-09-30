//! Shared MCP server mechanics for provider-backed generation servers.
//!
//! The crate keeps provider-neutral concerns out of individual adapters:
//! task records, webhook waiters, resource subscriptions, URI conventions,
//! shared artifact and usage contract types, and the small provider trait that
//! normalizes catalog and prediction behavior.

/// Canonical Veoveo hosted MCP server contract revision.
pub const HOSTED_MCP_CONTRACT_REVISION: &str = "veoveo.ai/hosted-mcp/v3";

pub mod access;
pub mod agent_management;
pub mod agents;
#[cfg(feature = "analytics")]
pub mod analytics;
pub mod artifact_service;
pub mod bootstrap;
pub mod catalog;
pub mod deployment;
pub mod docs;
pub mod gateway;
pub mod host;
pub mod internal_auth;
pub mod pagination;
pub mod protocol;
pub mod provider;
pub mod server_contract;
pub mod subscriptions;
pub mod tasks;
pub mod telemetry;
pub mod transport;
pub mod uri;
pub mod usage;
pub mod waiters;
pub mod work_context;
pub mod workspace;

pub use access::{
    AccessDecision, AccessRequest, GroupMembership, GroupRole, decide, grant_level_for_caller,
    mac_satisfied, role_in_group,
};
pub use agents::{
    AgentConversationEntry, AgentConversationEntryState, AgentConversationRole,
    AgentConversationView, AgentInputRequestDecision, AgentInputRequestView,
    AgentOperatorMessageRequest, AgentWakeReceipt,
};
#[cfg(feature = "analytics")]
pub use analytics::{DuckDbAnalytics, SharedDuckDbConnection, open_duckdb};
pub use artifact_service::upload::*;
pub use artifact_service::{
    ArtifactAccessRequest, ArtifactAccessRequestDecision, ArtifactAccessRequestId,
    ArtifactAccessRequestPage, ArtifactAccessRequestScope, ArtifactAccessRequestState,
    ArtifactPage, ArtifactPlane, ArtifactPlaneError, ArtifactReadAuthority,
    ArtifactReadCapabilityId, ArtifactReadCapabilityScope, ArtifactReadCapabilitySecret,
    ArtifactTaskId, ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret,
    ArtifactWriteIdempotencyKey, CreateArtifactAccessRequest, CreateArtifactShareLinkRequest,
    DecideArtifactAccessRequest, GrantList, IssueArtifactReadCapabilityRequest,
    IssueArtifactWriteCapabilityRequest, IssuedArtifactReadCapability,
    IssuedArtifactWriteCapability, ListArtifactAccessRequests, ListArtifactsRequest,
    MAX_ARTIFACT_PUT_DESCRIPTOR_BYTES, PlaneCaller, PutArtifactRequest, PutGrantRequest,
    RedeemArtifactWriteCapabilityRequest, SetArtifactReleaseStateRequest, StreamArtifactRequest,
};
pub use bootstrap::{
    SERVER_BOOTSTRAP_FLAG, SERVER_BOOTSTRAP_ISSUER, SERVER_BOOTSTRAP_MOUNT_PATH,
    SERVER_BOOTSTRAP_VALIDATE_COMMAND, ServerBootstrapDocument, ServerBootstrapError,
    server_bootstrap_principal,
};
pub use catalog::{
    GATEWAY_DISCOVERY_DEGRADATION_META_KEY, GatewayDiscoveryDegradation, GatewayDiscoveryFailure,
    GatewayDiscoveryFailureCode, GatewayDiscoverySurface,
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
pub use gateway::{
    APP_RESOURCE_DEPENDENCIES_META_KEY, APP_TOOL_DEPENDENCIES_META_KEY, AccessTokenSubject,
    AppResourceDependency, AppResourceOperation, AppToolDependency, AppToolImport,
    ArtifactAudience, AuthMethod, AuthMode, AuthOutcome, AuthReasonCode,
    AuthorizationServerEndpoint, AuthorizationServerId, CanonicalTaskId,
    CertificateAuthorityFilePath, CertificateAuthoritySource, CompatibilityHelperId,
    CompletionExposure, ConsoleBootstrap, ConsoleInstallation, ConsoleSession, ConsoleTenant,
    DataLabelDefinition, DiscoveryFailureMode, Exposure, GatewayAction,
    GatewayAuthorizationCodeRecord, GatewayAuthorizationRequest, GatewayControlPlane,
    GatewayControlPlaneError, GatewayControlPlaneRevision, GatewayControlPlaneRevisionId,
    GatewayControlPlaneRevisionSource, GatewayJwtRevocation, GatewayJwtRevocationAdminStatus,
    GatewayJwtRevocationApplyResult, GatewayJwtRevocationPruneResult, GatewayJwtRevocationRequest,
    GatewayProfile, GatewayProfileId, GatewayRefreshFamilyId, GatewayRefreshGrant,
    GatewayRefreshRevocationRequest, GatewayResourceProjection, GatewayResourceSubscription,
    GatewayToolName, HttpsUrl, IdentityProvider, IdentityProviderClaimMapping,
    IdentityProviderEndpoint, IdentityProviderId, IdentityProviderOidcClientRegistration,
    IdentityProviderSubjectClaim, IdentityProviderTenantClaim, IdentityProviderTenantClaimMapping,
    JwksFilePath, JwksSource, JwtId, LocalToolName, MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION,
    MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION, McpMethodName, McpSurfaceCapabilities,
    McpSurfaceCapability, MountPath, OAuthAuthorizationCode, OAuthClientAuthMethod, OAuthClientId,
    OAuthClientRegistration, OAuthClientSurface, OAuthEndpointUrl, OAuthGrantType,
    OAuthRedirectUri, OAuthRefreshToken, OAuthStateValue, OAuthTokenTypeHint, OidcClientAuthMethod,
    OidcClientId, OidcClientRegistrationId, OidcNonce, OpaqueTaskId, OwnedRoute, OwnedRoutePurpose,
    PkceCodeChallenge, PkceCodeChallengeMethod, PkceCodeVerifier, PlatformCapabilityId,
    PolicyDecision, PolicyEffect, PolicyReasonCode, PolicyRule, PolicyRuleId, PolicySet,
    PolicyTarget, Principal, PrincipalAssurance, PrincipalAuditAttributes, PrincipalDisplayName,
    PrincipalKind, ProfileServerExposure, PromptName, ProtectedResourceId, ProtectedResourceName,
    RecordingApplicationId, RecordingDatasetName, RecordingIngestResource, RecordingIngestStreamId,
    RecordingProducerBlueprintPolicy, RecordingProducerId, RecordingProducerQuotas,
    RecordingProducerRegistration, RecordingRetentionPolicy, ResourceAuthorizationServer,
    ResourceProjectionMode, ResourceSelector, ResourceUriPrefix, ResourceUriTemplate,
    SecretLocator, SecretOwner, SecretPurpose, SecretReference, SecretReferenceId, SecretSource,
    ServerManifest, ServerSlug, TaskExposure, TenantDefinition, TokenIssuer, TokenSubject, TraceId,
    UpstreamEndpoint, UpstreamTransport, UpstreamTransportSecurity, UpstreamUrl,
    console_bootstrap_schema,
};
pub use host::{
    HostAuthority, host_authority_is_allowed, parse_allowed_host_authority,
    parse_request_host_authority, public_allowed_hosts,
};
pub use internal_auth::{
    DEFAULT_GATEWAY_INTERNAL_SIGNING_KEY_ID, GATEWAY_INTERNAL_TOKEN_ISSUER,
    GATEWAY_ROUTING_REQUIRED, GatewayInternalIdentity, GatewayInternalResourceIdentity,
    GatewayInternalResourceTokenVerifier, GatewayInternalSigningKey, GatewayInternalTokenIssuer,
    GatewayInternalTokenVerifier, GatewayInternalTrustBundle, GatewayRequestContext,
    InternalTokenError, IssuedGatewayInternalResourceToken, IssuedGatewayInternalToken,
    VerifiedArtifactUploadIdentity,
};
pub use pagination::{Page, PaginationError, paginate};
pub use protocol::{
    MAX_BAGGAGE_BYTES, MAX_TRACESTATE_BYTES, PRIVATE_CATALOG_TTL_MS, PRIVATE_RESOURCE_TTL_MS,
    PUBLIC_IMMUTABLE_TTL_MS, final_protocol_versions, private_resource_response,
    sanitized_request_meta, trace_id_from_traceparent,
};
pub use provider::Provider;
pub use subscriptions::{
    ResourceListObservers, ResourceUpdate, SubscriptionHub, accepted_subscription_filter,
    accepted_task_subscription_filter, listen_resources, receive_resource_list_change,
    receive_resource_update, send_resource_update,
};
pub use tasks::{
    GATEWAY_TASK_RESOURCE_TEMPLATE, GatewayTaskStatus, GatewayTaskStatusDocument,
    GatewayTaskStatusKind, RELATED_TASK_META_KEY, gateway_task_resource_uri, notify_progress,
    now_utc, parse_gateway_task_resource_uri, related_task_meta, set_related_task_meta,
};
pub use telemetry::{TelemetryGuard, init_server_telemetry};
pub use transport::{
    MAX_MCP_RESPONSE_BYTES, bound_serialized_jsonrpc_response,
    canonical_streamable_http_server_config, enforce_serialized_mcp_response,
    stateless_session_manager,
};
pub use uri::{
    ServerResourceUri, ServerResourceUriError, ServerResourceUris, parse_server_doc_uri,
};
pub use usage::{UsageKind, UsageRecord, UsageReport};
pub use waiters::WebhookWaiters;
pub use work_context::{WorkContextDefinition, WorkContextMembershipRule};

/// Unified platform audit contract, also used by non-MCP producers.
pub mod audit {
    pub use veoveo_audit_contract::*;
}
