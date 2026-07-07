//! Authenticating a domain server's forwarded gateway identity.
//!
//! The plane does not mint tokens and domain servers do not hold the signing
//! secret. A domain server forwards the gateway-signed bearer it received
//! (audienced to that server); the plane verifies it against the set of known
//! upstream slugs and reads the embedded principal.

use std::collections::BTreeSet;

use veoveo_mcp_contract::gateway::ServerSlug;
use veoveo_mcp_contract::internal_auth::{
    GatewayInternalTokenVerifier, InternalTokenSecret,
};
use veoveo_mcp_contract::{ArtifactPlaneError, PlaneCaller, TokenIssuer};

/// Verifies forwarded gateway tokens and builds the [`PlaneCaller`].
#[derive(Clone)]
pub struct PlaneAuthenticator {
    verifier: GatewayInternalTokenVerifier,
}

impl PlaneAuthenticator {
    pub fn new(
        issuer: TokenIssuer,
        allowed_audiences: Vec<ServerSlug>,
        secret: InternalTokenSecret,
    ) -> Self {
        Self {
            verifier: GatewayInternalTokenVerifier::new_for_audiences(
                issuer,
                allowed_audiences,
                secret,
            ),
        }
    }

    /// Verify a `Bearer` token value and produce the acting caller. Group
    /// memberships are empty until the signed identity carries `(group, role)`
    /// pairs (P3); until then only user and owner grants resolve.
    pub fn authenticate(&self, bearer_token: &str) -> Result<PlaneCaller, ArtifactPlaneError> {
        let identity = self
            .verifier
            .verify(bearer_token)
            .map_err(|_| ArtifactPlaneError::Unauthenticated)?;
        Ok(PlaneCaller {
            bearer_token: bearer_token.to_string(),
            identity,
            memberships: BTreeSet::new(),
        })
    }
}
