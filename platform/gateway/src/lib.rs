pub mod auth;
mod catalog;
mod catalog_admission;
pub use catalog_admission::{CatalogAdmission, GatewayCatalogAdmission};
mod control_store;
pub mod mcp;
mod mcp_support;
mod metadata;
pub mod oauth_clients;
mod policy;
mod principal_audit;
pub mod request_observation;
pub mod secrets;
pub mod state;
#[cfg(test)]
#[path = "../../../testing/fixtures/store.rs"]
mod test_store;
mod tool_name;

pub use auth::{
    AuthError, AuthenticatedSubject, BearerToken, ClientAssertionConfig, ClientAssertionVerifier,
    IdJagConfig, IdJagVerifier, JwtAuthConfig, JwtVerifier, OidcIdTokenConfig, OidcIdTokenVerifier,
    VerifiedAccessToken, VerifiedClientAssertion, VerifiedIdJag, VerifiedOidcIdentity,
};
pub use catalog::{
    GatewayAuthorityError, GatewayCatalog, GatewayCatalogHandle, GatewayCatalogSnapshot,
};
pub use control_store::{
    GatewayControlPlaneRevisionHead, GatewayControlStore, new_gateway_control_plane_revision_id,
};
pub use mcp::{
    GatewayMcp, GatewayServerHealth, GatewayServerHealthState, GatewayUpstreamHttpClientPool,
    UpstreamClientKey, probe_gateway_server_health, upstream_client_builder, upstream_client_key,
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
    GatewayRefreshDeliveryWindow, GatewayRefreshExchange, GatewayRefreshIssueRequest,
    GatewayRefreshRotationRequest, GatewayState, IssuedGatewayRefreshToken,
    REFRESH_TOKEN_TTL_SECONDS, RefreshTokenDeliveryCipher,
};
pub use tool_name::{GatewayNameError, GatewayToolProjection};

pub mod audit;

#[cfg(test)]
extern crate self as veoveo_mcp_gateway;
#[cfg(test)]
#[path = "../../../testing/fixtures/catalog_admission.rs"]
mod test_catalog_admission;
