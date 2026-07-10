use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayJwtRevocation {
    pub profile: GatewayProfileId,
    pub issuer: TokenIssuer,
    pub jwt_id: JwtId,
    pub revoked_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayJwtRevocationRequest {
    pub profile: GatewayProfileId,
    pub issuer: TokenIssuer,
    pub jwt_id: JwtId,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GatewayJwtRevocationAdminStatus {
    Revoked,
    Pruned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayJwtRevocationApplyResult {
    pub status: GatewayJwtRevocationAdminStatus,
    pub revocation: GatewayJwtRevocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayJwtRevocationPruneResult {
    pub status: GatewayJwtRevocationAdminStatus,
    pub deleted: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayAuthorizationRequest {
    pub idp_state: OAuthStateValue,
    pub profile: GatewayProfileId,
    pub oauth_client_id: OAuthClientId,
    pub oidc_client: OidcClientRegistrationId,
    pub redirect_uri: OAuthRedirectUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_state: Option<OAuthStateValue>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub requested_scopes: BTreeSet<ScopeName>,
    pub code_challenge: PkceCodeChallenge,
    pub code_challenge_method: PkceCodeChallengeMethod,
    pub idp_code_verifier: PkceCodeVerifier,
    pub idp_code_challenge: PkceCodeChallenge,
    pub idp_code_challenge_method: PkceCodeChallengeMethod,
    pub nonce: OidcNonce,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayAuthorizationCodeRecord {
    pub code: OAuthAuthorizationCode,
    pub profile: GatewayProfileId,
    pub oauth_client_id: OAuthClientId,
    pub oidc_client: OidcClientRegistrationId,
    pub redirect_uri: OAuthRedirectUri,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_state: Option<OAuthStateValue>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub scopes: BTreeSet<ScopeName>,
    pub code_challenge: PkceCodeChallenge,
    pub code_challenge_method: PkceCodeChallengeMethod,
    pub principal: Principal,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayRefreshGrant {
    pub family_id: GatewayRefreshFamilyId,
    pub authorization_server: AuthorizationServerId,
    pub profile: GatewayProfileId,
    pub oauth_client_id: OAuthClientId,
    pub principal: Principal,
    pub scopes: BTreeSet<ScopeName>,
    pub generation: u64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OAuthTokenTypeHint {
    RefreshToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayRefreshRevocationRequest {
    pub token: OAuthRefreshToken,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_type_hint: Option<OAuthTokenTypeHint>,
    pub client_id: OAuthClientId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<ProtectedResourceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PkceCodeChallengeMethod {
    #[serde(rename = "S256")]
    S256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayResourceProjection {
    pub server: ServerSlug,
    pub gateway_uri: ResourceUri,
    pub upstream_uri: ResourceUri,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayResourceSubscription {
    pub profile: GatewayProfileId,
    pub owner: PrincipalId,
    pub upstream_server: ServerSlug,
    pub resource_uri: ResourceUri,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
