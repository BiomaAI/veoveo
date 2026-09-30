//! Authentication outcomes shared by the gateway and its audit record.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthOutcome {
    Allow,
    Deny,
}

impl AuthOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    BearerJwt,
    OidcAuthorizationCodePkce,
    RefreshToken,
    ClientCredentialsPrivateKeyJwt,
    EnterpriseManagedIdJag,
}

impl AuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BearerJwt => "bearer_jwt",
            Self::OidcAuthorizationCodePkce => "oidc_authorization_code_pkce",
            Self::RefreshToken => "refresh_token",
            Self::ClientCredentialsPrivateKeyJwt => "client_credentials_private_key_jwt",
            Self::EnterpriseManagedIdJag => "enterprise_managed_id_jag",
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AuthReasonCode {
    AuthAllow,
    MissingAuthorizationHeader,
    InvalidAuthorizationHeader,
    UnknownIdentityProvider,
    UnknownAuthorizationServer,
    IdentityProviderUnavailable,
    AuthorizationServerUnavailable,
    InvalidAuthConfig,
    InvalidBearerToken,
    InvalidAuthorizationRequest,
    InvalidAuthorizationCode,
    InvalidRefreshToken,
    RefreshTokenDuplicateDelivery,
    RefreshTokenRevoked,
    RefreshTokenReplay,
    InvalidPkce,
    InvalidOidcIdToken,
    InvalidClient,
    UnsupportedGrantType,
    InvalidClientAssertion,
    ClientAssertionReplay,
    InvalidIdentityAssertion,
    IdentityAssertionReplay,
    InvalidScope,
    TokenSigningKeyUnavailable,
    TokenRevoked,
    AuthStateUnavailable,
    PolicyDenied,
}

impl AuthReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AuthAllow => "auth_allow",
            Self::MissingAuthorizationHeader => "missing_authorization_header",
            Self::InvalidAuthorizationHeader => "invalid_authorization_header",
            Self::UnknownIdentityProvider => "unknown_identity_provider",
            Self::UnknownAuthorizationServer => "unknown_authorization_server",
            Self::IdentityProviderUnavailable => "identity_provider_unavailable",
            Self::AuthorizationServerUnavailable => "authorization_server_unavailable",
            Self::InvalidAuthConfig => "invalid_auth_config",
            Self::InvalidBearerToken => "invalid_bearer_token",
            Self::InvalidAuthorizationRequest => "invalid_authorization_request",
            Self::InvalidAuthorizationCode => "invalid_authorization_code",
            Self::InvalidRefreshToken => "invalid_refresh_token",
            Self::RefreshTokenDuplicateDelivery => "refresh_token_duplicate_delivery",
            Self::RefreshTokenRevoked => "refresh_token_revoked",
            Self::RefreshTokenReplay => "refresh_token_replay",
            Self::InvalidPkce => "invalid_pkce",
            Self::InvalidOidcIdToken => "invalid_oidc_id_token",
            Self::InvalidClient => "invalid_client",
            Self::UnsupportedGrantType => "unsupported_grant_type",
            Self::InvalidClientAssertion => "invalid_client_assertion",
            Self::ClientAssertionReplay => "client_assertion_replay",
            Self::InvalidIdentityAssertion => "invalid_identity_assertion",
            Self::IdentityAssertionReplay => "identity_assertion_replay",
            Self::InvalidScope => "invalid_scope",
            Self::TokenSigningKeyUnavailable => "token_signing_key_unavailable",
            Self::TokenRevoked => "token_revoked",
            Self::AuthStateUnavailable => "auth_state_unavailable",
            Self::PolicyDenied => "policy_denied",
        }
    }
}
