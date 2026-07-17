pub mod auth;
mod catalog;
mod control_store;
pub mod mcp;
mod mcp_support;
mod metadata;
mod policy;
mod principal_audit;
pub mod secrets;
pub mod state;
mod tool_name;

pub use auth::{
    AuthError, AuthenticatedSubject, BearerToken, ClientAssertionConfig, ClientAssertionVerifier,
    IdJagConfig, IdJagVerifier, JwtAuthConfig, JwtVerifier, OidcIdTokenConfig, OidcIdTokenVerifier,
    VerifiedClientAssertion, VerifiedIdJag, VerifiedOidcIdentity,
};
pub use catalog::{GatewayCatalog, GatewayCatalogHandle, GatewayCatalogSnapshot};
pub use control_store::{
    GatewayControlPlaneRevisionHead, GatewayControlStore, new_gateway_control_plane_revision_id,
};
pub use mcp::{
    FinalTaskClient, GatewayMcp, GatewayServerHealth, GatewayServerHealthState,
    GatewayTaskExtension, build_upstream_http_client, probe_gateway_server_health,
};
pub use metadata::{
    AuthorizationExtensionMetadata, AuthorizationServerMetadata, GatewayMetadataError,
    ProtectedResourceMetadata, www_authenticate_challenge,
};
pub use policy::{
    PolicyRequest, RecordingIngestPolicyDecision, RecordingIngestPolicyRequest, mcp_method_name,
    resource_scheme_from_uri,
};
pub use principal_audit::{merge_principal_audit_metadata, principal_audit_metadata};
pub use secrets::{GatewaySecretResolver, ResolvedSecretString, SecretResolverError};
pub use state::{
    GatewayAuditCounts, GatewayAuditRetentionSummary, GatewayAuthAuditMetadataSummary,
    GatewayAuthAuditMethodSummary, GatewayAuthAuditReasonSummary, GatewayRefreshDeliveryWindow,
    GatewayRefreshExchange, GatewayRefreshRotationRequest, GatewayState, IssuedGatewayRefreshToken,
    REFRESH_TOKEN_TTL_SECONDS, RefreshTokenDeliveryCipher,
};
pub use tool_name::{GatewayNameError, GatewayToolProjection};
