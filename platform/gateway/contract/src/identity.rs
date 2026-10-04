//! Neutral installation identities shared by HTTP and authorization adapters.

pub use veoveo_types::OAuthClientId;

/// Resource authorization server id that issues profile-scoped MCP access tokens.
#[veoveo_types::id(text(veoveo_types::identifier_syntax::PathIdProfile))]
pub struct AuthorizationServerId(String);

/// Reference to a secret managed outside control data.
#[veoveo_types::id(text(veoveo_types::identifier_syntax::TokenTextProfile))]
pub struct SecretReferenceId(String);

/// OAuth protected-resource URI for an MCP profile or platform data plane.
#[veoveo_types::id(text(veoveo_types::identifier_syntax::ClaimTextProfile))]
pub struct ProtectedResourceId(String);

/// Installation-local name for an OAuth protected resource.
#[veoveo_types::id(text(veoveo_types::identifier_syntax::PathIdProfile))]
pub struct ProtectedResourceName(String);

/// External secret locator. This is a reference path, not a secret value.
#[veoveo_types::id(text(veoveo_types::identifier_syntax::ClaimTextProfile))]
pub struct SecretLocator(String);
