use std::{fmt, str::FromStr};
pub use veoveo_types::{
    CanonicalTaskId, GatewayProfileId, GatewayRefreshFamilyId, LocalToolName, OAuthClientId,
    PromptName, ServerSlug, TokenIssuer, TokenSubject,
};
use veoveo_types::{
    IdentifierError,
    identifier_syntax::{ClaimTextProfile, PathIdProfile, TokenTextProfile},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::wire::{
    validate_compatibility_helper_id, validate_oauth_authorization_code,
    validate_oauth_state_value, validate_pkce_code_token, validate_principal_display_name,
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

/// Human-readable label for the authenticated principal. It is display metadata, never an authorization identity.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct PrincipalDisplayName(String);

impl PrincipalDisplayName {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_principal_display_name(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for PrincipalDisplayName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for PrincipalDisplayName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for PrincipalDisplayName {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl FromStr for PrincipalDisplayName {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl From<PrincipalDisplayName> for String {
    fn from(value: PrincipalDisplayName) -> Self {
        value.0
    }
}

/// Policy rule identifier used for decision evidence.
#[veoveo_types::id(text(TokenTextProfile))]
pub struct PolicyRuleId(String);

/// Explicit compatibility helper id exposed to limited MCP clients, for example `media.models`.
#[veoveo_types::id(text(GatewayCompatibilityHelperIdProfile))]
pub struct CompatibilityHelperId(String);

/// Gateway registration id for its OIDC client relationship with an enterprise identity provider.
#[veoveo_types::id(text(PathIdProfile))]
pub struct OidcClientRegistrationId(String);

/// OIDC client id assigned to the gateway by an enterprise identity provider.
#[veoveo_types::id(text(ClaimTextProfile))]
pub struct OidcClientId(String);

/// OIDC nonce bound to an enterprise identity-provider authorization request.
#[veoveo_types::id(text(GatewayOauthStateValueProfile))]
pub struct OidcNonce(String);

/// JWT id used for replay protection or revocation tracking.
#[veoveo_types::id(text(ClaimTextProfile))]
pub struct JwtId(String);

/// Opaque OAuth state value stored for browser authorization continuity.
#[veoveo_types::id(text(GatewayOauthStateValueProfile))]
pub struct OAuthStateValue(String);

/// Gateway-issued OAuth authorization code exchanged once for a profile access token.
#[veoveo_types::id(text(GatewayOauthAuthorizationCodeProfile))]
pub struct OAuthAuthorizationCode(String);

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

/// PKCE code challenge bound to a gateway-issued authorization code.
#[veoveo_types::id(text(GatewayPkceTokens))]
pub struct PkceCodeChallenge(String);

/// PKCE code verifier presented to the gateway token endpoint.
#[veoveo_types::id(text(GatewayPkceTokens))]
pub struct PkceCodeVerifier(String);

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
pub struct GatewayOauthStateValueProfile;
impl IdProfile for GatewayOauthStateValueProfile {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_oauth_state_value(value));
}
#[doc(hidden)]
pub struct GatewayOauthAuthorizationCodeProfile;
impl IdProfile for GatewayOauthAuthorizationCodeProfile {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_oauth_authorization_code(value));
}

#[doc(hidden)]
pub struct GatewayRefreshTokens;
impl IdProfile for GatewayRefreshTokens {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_refresh_token(value));
}

#[doc(hidden)]
pub struct GatewayPkceTokens;
impl IdProfile for GatewayPkceTokens {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_pkce_code_token(value));
}

fn validate_refresh_token(value: &str) -> Result<(), IdentifierError> {
    validate_oauth_authorization_code(value).map_err(|_| {
        IdentifierError::new(
            "[REDACTED]",
            "must be 43 to 128 bytes containing only ASCII letters, digits, hyphen, period, underscore, or tilde",
        )
    })
}
