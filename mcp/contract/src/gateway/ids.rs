use std::fmt;
pub use veoveo_types::{
    CanonicalTaskId, GatewayProfileId, GatewayRefreshFamilyId, LocalToolName, OAuthClientId,
    PromptName, ServerSlug, TokenIssuer, TokenSubject,
};
use veoveo_types::{
    IdentifierError,
    identifier_syntax::{ClaimTextProfile, PathIdProfile, TokenTextProfile},
};

use super::wire::validate_compatibility_helper_id;
use veoveo_gateway_contract::validate_oauth_authorization_code;
pub use veoveo_gateway_contract::{
    JwtId, OAuthAuthorizationCode, OAuthStateValue, OidcClientRegistrationId, OidcNonce,
    PkceCodeChallenge, PkceCodeVerifier, PrincipalDisplayName,
};

/// Configured identity provider id used by gateway profiles.
#[veoveo_types::id(text(PathIdProfile))]
pub struct IdentityProviderId(String);

/// Artifact-service audience admitted for one hosted server.
#[veoveo_types::id(text(PathIdProfile))]
pub struct ArtifactAudience(String);

/// Installation platform capability required by one hosted server.
#[veoveo_types::id(text(PathIdProfile))]
pub struct PlatformCapabilityId(String);

/// Policy rule identifier used for decision evidence.
#[veoveo_types::id(text(TokenTextProfile))]
pub struct PolicyRuleId(String);

/// Explicit compatibility helper id exposed to limited MCP clients, for example `media.models`.
#[veoveo_types::id(text(GatewayCompatibilityHelperIdProfile))]
pub struct CompatibilityHelperId(String);

/// OIDC client id assigned to the gateway by an enterprise identity provider.
#[veoveo_types::id(text(ClaimTextProfile))]
pub struct OidcClientId(String);

/// Opaque, rotating OAuth refresh token. Only its SHA-256 digest is persisted.
#[veoveo_types::id(text(GatewayRefreshTokens), secret, ordered)]
pub struct OAuthRefreshToken(String);

impl OAuthRefreshToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for OAuthRefreshToken {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Debug for OAuthRefreshToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OAuthRefreshToken([REDACTED])")
    }
}

impl fmt::Display for OAuthRefreshToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

/// Request trace/correlation id used in audit and runtime state.
#[veoveo_types::id(text(TokenTextProfile))]
pub struct TraceId(String);

/// Provider-server task identifier used by the final MCP task identity.
#[veoveo_types::id(text(TokenTextProfile))]
pub struct OpaqueTaskId(String);

/// Durable gateway control-plane revision id.
#[veoveo_types::id(text(TokenTextProfile))]
pub struct GatewayControlPlaneRevisionId(String);

/// MCP JSON-RPC method name used in policy and audit events.
#[veoveo_types::id(text(TokenTextProfile))]
pub struct McpMethodName(String);

use veoveo_types::{IdProfile, IdProfileSpec};

#[doc(hidden)]
pub struct GatewayCompatibilityHelperIdProfile;
impl IdProfile for GatewayCompatibilityHelperIdProfile {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_compatibility_helper_id(value));
}
#[doc(hidden)]
pub struct GatewayRefreshTokens;
impl IdProfile for GatewayRefreshTokens {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_refresh_token(value));
}

fn validate_refresh_token(value: &str) -> Result<(), IdentifierError> {
    validate_oauth_authorization_code(value).map_err(|_| {
        IdentifierError::new(
            "[REDACTED]",
            "must be 43 to 128 bytes containing only ASCII letters, digits, hyphen, period, underscore, or tilde",
        )
    })
}
