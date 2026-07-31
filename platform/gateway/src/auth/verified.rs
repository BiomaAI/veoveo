use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use veoveo_mcp_contract::{
    AccessTokenSubject, InvocationAuthority, JwtId, OAuthClientId, Principal, PrincipalDisplayName,
    ScopeName,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAccessToken {
    pub access_token: AccessTokenSubject,
    pub principal: Principal,
    pub principal_display_name: Option<PrincipalDisplayName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSubject {
    pub access_token: AccessTokenSubject,
    pub principal: Principal,
    pub actor: Principal,
    pub principal_display_name: Option<PrincipalDisplayName>,
    pub authority: InvocationAuthority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClientAssertion {
    pub client_id: OAuthClientId,
    pub jwt_id: JwtId,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdJag {
    pub client_id: OAuthClientId,
    pub principal: Principal,
    pub scopes: BTreeSet<ScopeName>,
    pub jwt_id: JwtId,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedOidcIdentity {
    pub principal: Principal,
    pub principal_display_name: PrincipalDisplayName,
    pub expires_at: DateTime<Utc>,
}
