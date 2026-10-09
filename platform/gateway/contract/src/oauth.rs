//! OAuth continuity values shared by gateway storage and protocol adapters.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use url::{Host, Url};
use veoveo_types::identifier_syntax::{ClaimTextProfile, PathIdProfile, validate_token_text};
use veoveo_types::{IdProfile, IdProfileSpec, IdentifierError};
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

/// Gateway registration id for its OIDC client relationship with an enterprise identity provider.
#[veoveo_types::id(text(PathIdProfile))]
pub struct OidcClientRegistrationId(String);

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

/// PKCE code challenge bound to a gateway-issued authorization code.
#[veoveo_types::id(text(GatewayPkceTokens))]
pub struct PkceCodeChallenge(String);

/// PKCE code verifier presented to the gateway token endpoint.
#[veoveo_types::id(text(GatewayPkceTokens))]
pub struct PkceCodeVerifier(String);

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
pub struct GatewayPkceTokens;
impl IdProfile for GatewayPkceTokens {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> =
        IdProfileSpec::text(|value, _| validate_pkce_code_token(value));
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct OAuthRedirectUri(String);

impl OAuthRedirectUri {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_oauth_redirect_uri(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for OAuthRedirectUri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for OAuthRedirectUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for OAuthRedirectUri {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl FromStr for OAuthRedirectUri {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value.to_string())
    }
}

impl From<OAuthRedirectUri> for String {
    fn from(value: OAuthRedirectUri) -> Self {
        value.0
    }
}

pub fn validate_oauth_state_value(value: &str) -> Result<(), IdentifierError> {
    validate_token_text(value)?;
    if value.len() > 512 {
        return Err(IdentifierError::new(value, "must be at most 512 bytes"));
    }
    Ok(())
}

pub fn validate_oauth_authorization_code(value: &str) -> Result<(), IdentifierError> {
    validate_pkce_code_token(value)
}

pub fn validate_pkce_code_token(value: &str) -> Result<(), IdentifierError> {
    if !(43..=128).contains(&value.len()) {
        return Err(IdentifierError::new(value, "must be 43 to 128 bytes"));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~'))
    {
        return Err(IdentifierError::new(
            value,
            "must contain only ASCII letters, digits, hyphen, period, underscore, or tilde",
        ));
    }
    Ok(())
}

pub fn validate_principal_display_name(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() || value.trim() != value {
        return Err(IdentifierError::new(
            value,
            "must be non-empty without leading or trailing whitespace",
        ));
    }
    if value.len() > 256 {
        return Err(IdentifierError::new(
            value,
            "must be at most 256 UTF-8 bytes",
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(IdentifierError::new(
            value,
            "must not contain control characters",
        ));
    }
    Ok(())
}

pub fn validate_oauth_redirect_uri(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    let url = Url::parse(value).map_err(|_| IdentifierError::new(value, "must be a valid URL"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(IdentifierError::new(value, "must not contain userinfo"));
    }
    if url.fragment().is_some() {
        return Err(IdentifierError::new(value, "must not contain a fragment"));
    }
    match url.scheme() {
        "https" => {
            if url.host().is_none() {
                return Err(IdentifierError::new(value, "must include a host"));
            }
            Ok(())
        }
        "http" => {
            let is_loopback = match url.host() {
                Some(Host::Domain(host)) => host == "localhost",
                Some(Host::Ipv4(addr)) => addr.is_loopback(),
                Some(Host::Ipv6(addr)) => addr.is_loopback(),
                None => false,
            };
            if is_loopback && url.port().is_some_and(|port| port != 0) {
                return Ok(());
            }
            Err(IdentifierError::new(
                value,
                "http:// redirect URIs must use loopback host and explicit non-zero port",
            ))
        }
        _ => Err(IdentifierError::new(
            value,
            "must use https:// or local loopback http://",
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum PkceCodeChallengeMethod {
    #[vocabulary(rename = "S256")]
    S256,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OAuthGrantType {
    AuthorizationCodePkce,
    RefreshToken,
    ClientCredentials,
    EnterpriseManagedAuthorization,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OAuthClientAuthMethod {
    None,
    PrivateKeyJwt,
    ClientSecretBasic,
    ClientSecretPost,
    TlsClientAuth,
}

impl OAuthClientAuthMethod {
    pub fn requires_secret(&self) -> bool {
        matches!(self, Self::ClientSecretBasic | Self::ClientSecretPost)
    }

    pub fn requires_jwks(&self) -> bool {
        matches!(self, Self::PrivateKeyJwt)
    }
}
