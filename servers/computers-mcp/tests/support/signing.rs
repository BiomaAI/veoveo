use crate::support;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::json;
use veoveo_mcp_contract::*;
pub struct Signing {
    issuer: GatewayInternalTokenIssuer,
    pub verifier: GatewayInternalTokenVerifier,
}
impl Signing {
    pub fn new() -> Self {
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        let trust = GatewayInternalTrustBundle::from_json(&json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key_raw()),"alg":"EdDSA","use":"sig","kid":"fixture"}]}).to_string()).unwrap();
        let issuer = TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap();
        Self {
            issuer: GatewayInternalTokenIssuer::new(
                issuer.clone(),
                GatewayInternalSigningKey::new("fixture", key.serialize_der()).unwrap(),
            ),
            verifier: GatewayInternalTokenVerifier::new(
                issuer,
                ServerSlug::new("computers").unwrap(),
                trust,
            ),
        }
    }
    pub fn bearer(&self, subject: &str, audience: &str) -> String {
        self.bearer_until(
            subject,
            audience,
            chrono::Utc::now() + chrono::TimeDelta::minutes(2),
        )
    }
    pub fn bearer_until(
        &self,
        subject: &str,
        audience: &str,
        expires: chrono::DateTime<chrono::Utc>,
    ) -> String {
        self.identity(
            support::identity(&support::owner(subject)),
            audience,
            expires,
        )
    }
    pub fn identity(
        &self,
        id: GatewayInternalIdentity,
        audience: &str,
        expires: chrono::DateTime<chrono::Utc>,
    ) -> String {
        self.issuer
            .issue(
                id.profile,
                ServerSlug::new(audience).unwrap(),
                id.actor,
                id.authority,
                id.request_context,
                expires,
            )
            .unwrap()
            .bearer_token
    }
}
