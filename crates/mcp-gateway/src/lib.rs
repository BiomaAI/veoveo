pub mod auth;
mod catalog;
pub mod mcp;
mod mcp_support;
mod metadata;
mod policy;
pub mod secrets;
pub mod state;
mod tool_name;

pub use auth::{
    AuthError, AuthenticatedSubject, BearerToken, ClientAssertionConfig, ClientAssertionVerifier,
    IdJagConfig, IdJagVerifier, JwtAuthConfig, JwtVerifier, OidcIdTokenConfig, OidcIdTokenVerifier,
    VerifiedClientAssertion, VerifiedIdJag, VerifiedOidcIdentity,
};
pub use catalog::{GatewayCatalog, GatewayCatalogHandle, GatewayCatalogSnapshot};
pub use mcp::GatewayMcp;
pub use metadata::{
    AuthorizationExtensionMetadata, AuthorizationServerMetadata, GatewayMetadataError,
    ProtectedResourceMetadata, www_authenticate_challenge,
};
pub use policy::{PolicyRequest, mcp_method_name, resource_scheme_from_uri};
pub use secrets::{GatewaySecretResolver, ResolvedSecretString, SecretResolverError};
pub use state::{GatewayAuditCounts, GatewayAuditRetentionSummary, GatewayState};
pub use tool_name::{GatewayNameError, GatewayToolProjection};
