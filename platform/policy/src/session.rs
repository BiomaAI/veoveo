//! Pure current-session decision. Callers read the exact signed family ID from
//! their authoritative store and charge that read against their access deadline.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_mcp_contract::{
    AccessTokenSubject, AuthorizationServerId, GatewayProfileId, OAuthClientId, Principal,
    PrincipalId, ScopeName, TenantId, WorkContextId,
};

/// Read-only projection of the existing gateway refresh-family record. Display
/// metadata, delivery envelopes and database IDs are outside this decision.
#[derive(Deserialize)]
pub struct SessionFamilyAuthority {
    authorization_server: AuthorizationServerId,
    profile: GatewayProfileId,
    oauth_client_id: OAuthClientId,
    work_context: WorkContextId,
    principal_id: PrincipalId,
    tenant: Option<TenantId>,
    scopes: BTreeSet<ScopeName>,
    principal: StoredPrincipal,
    current_generation: u64,
    expires_at: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}
#[derive(Deserialize)]
struct StoredPrincipal {
    principal: Principal,
}

pub struct SessionRequest<'a> {
    pub profile: &'a GatewayProfileId,
    pub authorization_server: &'a AuthorizationServerId,
    pub token: &'a AccessTokenSubject,
    pub principal: &'a Principal,
    pub now: DateTime<Utc>,
}
impl SessionFamilyAuthority {
    /// Decode a trusted store record without introducing a store dependency into
    /// policy evaluation. Errors omit record contents, including principal data.
    pub fn from_stored(record: &impl Serialize) -> anyhow::Result<Self> {
        let value = serde_json::to_value(record)
            .map_err(|_| anyhow::anyhow!("session family encoding is invalid"))?;
        serde_json::from_value(value)
            .map_err(|_| anyhow::anyhow!("session family authority is invalid"))
    }
    pub fn allows(&self, request: SessionRequest<'_>) -> bool {
        let SessionRequest {
            profile,
            authorization_server,
            token,
            principal,
            now,
        } = request;
        // Access is bound to the family, not a rotating refresh-token generation.
        // The decoder still rejects negative/corrupt stored generations.
        let _ = self.current_generation;
        token.session_family.is_some() && self.revoked_at.is_none() && self.expires_at > now
            && self.principal_id == principal.id && self.tenant == principal.tenant
            && self.authorization_server == *authorization_server && self.profile == *profile
            && self.oauth_client_id == token.oauth_client_id && self.work_context == token.work_context
            && self.principal.principal.id == principal.id && self.principal.principal.kind == principal.kind
            // The retained principal has its original IdP issuer; the verified
            // access-token principal has the gateway issuer and the same identity.
            && token.issuer == principal.issuer && token.subject == principal.subject
            && self.principal.principal.subject == principal.subject
            && self.principal.principal.tenant == principal.tenant
            && token.scopes.is_subset(&self.scopes)
    }
}
