use std::{fmt, str::FromStr};
pub use veoveo_types::{
    CanonicalTaskId, GatewayProfileId, GatewayRefreshFamilyId, LocalToolName, OAuthClientId,
    PromptName, ServerSlug, TokenIssuer, TokenSubject,
};
use veoveo_types::{
    IdentifierError,
    identifier_syntax::{validate_claim_text, validate_path_id, validate_token_text},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::wire::{
    validate_compatibility_helper_id, validate_gateway_name, validate_oauth_authorization_code,
    validate_oauth_state_value, validate_pkce_code_token, validate_principal_display_name,
};

/// Configured identity provider id used by gateway profiles.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_path_id, error = IdentifierError)]
pub struct IdentityProviderId(String);

/// Gateway-scoped tool name after server namespace projection.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_gateway_name, error = IdentifierError)]
pub struct GatewayToolName(String);
impl GatewayToolName {
    /// Compose the gateway namespace from typed server and local tool names.
    pub fn from_parts(server: &ServerSlug, tool: &LocalToolName) -> Result<Self, IdentifierError> {
        Self::new(format!("{server}__{tool}"))
    }
}

/// Artifact-service audience admitted for one hosted server.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_gateway_name, error = IdentifierError)]
pub struct ArtifactAudience(String);

/// Installation platform capability required by one hosted server.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_path_id, error = IdentifierError)]
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
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_token_text, error = IdentifierError)]
pub struct PolicyRuleId(String);

/// Explicit compatibility helper id exposed to limited MCP clients, for example `media.models`.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_compatibility_helper_id, error = IdentifierError)]
pub struct CompatibilityHelperId(String);

/// Gateway registration id for its OIDC client relationship with an enterprise identity provider.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_path_id, error = IdentifierError)]
pub struct OidcClientRegistrationId(String);

/// OIDC client id assigned to the gateway by an enterprise identity provider.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_claim_text, error = IdentifierError)]
pub struct OidcClientId(String);

/// OIDC nonce bound to an enterprise identity-provider authorization request.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_oauth_state_value, error = IdentifierError)]
pub struct OidcNonce(String);

/// JWT id used for replay protection or revocation tracking.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_claim_text, error = IdentifierError)]
pub struct JwtId(String);

/// Opaque OAuth state value stored for browser authorization continuity.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_oauth_state_value, error = IdentifierError)]
pub struct OAuthStateValue(String);

/// Gateway-issued OAuth authorization code exchanged once for a profile access token.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_oauth_authorization_code, error = IdentifierError)]
pub struct OAuthAuthorizationCode(String);

/// Opaque, rotating OAuth refresh token. Only its SHA-256 digest is persisted.
#[derive(
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_refresh_token, error = IdentifierError, no_display)]
pub struct OAuthRefreshToken(String);

// Preserve admission while keeping rejected bearer material out of diagnostics.
fn validate_refresh_token(value: &str) -> Result<(), IdentifierError> {
    validate_oauth_authorization_code(value).map_err(|_| {
        IdentifierError::new(
            "[REDACTED]",
            "must be 43 to 128 bytes containing only ASCII letters, digits, hyphen, period, underscore, or tilde",
        )
    })
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
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_pkce_code_token, error = IdentifierError)]
pub struct PkceCodeChallenge(String);

/// PKCE code verifier presented to the gateway token endpoint.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_pkce_code_token, error = IdentifierError)]
pub struct PkceCodeVerifier(String);

/// Request trace/correlation id used in audit and runtime state.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_token_text, error = IdentifierError)]
pub struct TraceId(String);

/// Provider-server task identifier used by the final MCP task identity.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_token_text, error = IdentifierError)]
pub struct OpaqueTaskId(String);

/// Durable gateway control-plane revision id.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_token_text, error = IdentifierError)]
pub struct GatewayControlPlaneRevisionId(String);

/// MCP JSON-RPC method name used in policy and audit events.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_token_text, error = IdentifierError)]
pub struct McpMethodName(String);
