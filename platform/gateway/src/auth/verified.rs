use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use veoveo_mcp_contract::{AccessTokenSubject, JwtId, OAuthClientId, Principal, ScopeName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSubject {
    pub access_token: AccessTokenSubject,
    pub principal: Principal,
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
    pub expires_at: DateTime<Utc>,
}
