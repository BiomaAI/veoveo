use jsonwebtoken::{DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use veoveo_mcp_contract::JwtId;

use super::{
    claims::ClientAssertionClaims,
    config::ClientAssertionConfig,
    support::{
        AuthError, allowed_algorithms_for_header, ensure_jwt_crypto_provider, unix_timestamp,
        validate_jwk_algorithm,
    },
    verified::VerifiedClientAssertion,
};

#[derive(Debug, Clone)]
pub struct ClientAssertionVerifier {
    config: ClientAssertionConfig,
    jwks: JwkSet,
}

impl ClientAssertionVerifier {
    pub fn new(config: ClientAssertionConfig, jwks: JwkSet) -> Self {
        Self { config, jwks }
    }

    pub fn verify(&self, assertion: &str) -> Result<VerifiedClientAssertion, AuthError> {
        ensure_jwt_crypto_provider();
        if assertion.is_empty() || assertion.chars().any(char::is_whitespace) {
            return Err(AuthError::InvalidClientAssertion);
        }
        let header = decode_header(assertion).map_err(AuthError::Jwt)?;
        if !self.config.algorithms.contains(&header.alg) {
            return Err(AuthError::DisallowedAlgorithm(header.alg));
        }
        let key_id = header.kid.ok_or(AuthError::MissingKeyId)?;
        let jwk = self
            .jwks
            .find(&key_id)
            .ok_or_else(|| AuthError::UnknownKeyId(key_id.clone()))?;
        validate_jwk_algorithm(jwk.common.key_algorithm, header.alg)?;
        let key = DecodingKey::from_jwk(jwk).map_err(AuthError::Jwt)?;

        let algorithms = allowed_algorithms_for_header(&self.config.algorithms, header.alg)?;
        let mut validation = Validation::new(header.alg);
        validation.algorithms = algorithms;
        validation.validate_nbf = true;
        validation.set_issuer(&[self.config.client_id.as_str()]);
        validation.set_audience(&[self.config.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "jti"]);

        let data = decode::<ClientAssertionClaims>(assertion, &key, &validation)
            .map_err(AuthError::Jwt)?;
        let claims = data.claims;
        if claims.sub != self.config.client_id.as_str() {
            return Err(AuthError::ClientAssertionSubjectMismatch);
        }
        Ok(VerifiedClientAssertion {
            client_id: self.config.client_id.clone(),
            jwt_id: JwtId::new(claims.jti).map_err(AuthError::Claim)?,
            expires_at: unix_timestamp(claims.exp, "exp")?,
        })
    }
}
