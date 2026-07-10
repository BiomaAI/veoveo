use std::collections::BTreeSet;

use chrono::Utc;
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use veoveo_mcp_contract::{
    DataLabelId, GroupId, JwtId, OAuthClientId, Principal, PrincipalId, PrincipalKind, RoleId,
    TenantId, TokenIssuer, TokenSubject,
};

use super::{
    claims::{IdJagClaims, StringListClaim},
    config::IdJagConfig,
    principal::principal_assurances,
    support::{
        AuthError, allowed_algorithms_for_header, ensure_jwt_crypto_provider, unix_timestamp,
        validate_jwk_algorithm,
    },
    verified::VerifiedIdJag,
};

#[derive(Debug, Clone)]
pub struct IdJagVerifier {
    config: IdJagConfig,
    jwks: JwkSet,
}

impl IdJagVerifier {
    pub fn new(config: IdJagConfig, jwks: JwkSet) -> Self {
        Self { config, jwks }
    }

    pub fn verify(&self, assertion: &str) -> Result<VerifiedIdJag, AuthError> {
        ensure_jwt_crypto_provider();
        if assertion.is_empty() || assertion.chars().any(char::is_whitespace) {
            return Err(AuthError::InvalidIdentityAssertion);
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
        validation.set_issuer(&[self.config.issuer.as_str()]);
        validation.set_audience(&[self.config.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "jti"]);

        let data = decode::<IdJagClaims>(assertion, &key, &validation).map_err(AuthError::Jwt)?;
        let claims = data.claims;
        let resource = claims
            .resource
            .as_deref()
            .ok_or(AuthError::MissingIdentityAssertionResource)?;
        if resource != self.config.resource.as_str() {
            return Err(AuthError::InvalidIdentityAssertionResource);
        }
        let scopes = claims.scopes()?;
        if scopes.is_empty() {
            return Err(AuthError::MissingIdentityAssertionScope);
        }

        let issuer = TokenIssuer::new(claims.iss.clone()).map_err(AuthError::Claim)?;
        let subject = TokenSubject::new(claims.sub.clone()).map_err(AuthError::Claim)?;
        let client_id = OAuthClientId::new(claims.client_id.clone()).map_err(AuthError::Claim)?;
        let principal = Principal {
            id: PrincipalId::new(format!("{issuer}#{subject}")).map_err(AuthError::Claim)?,
            kind: PrincipalKind::User,
            issuer,
            subject,
            tenant: claims
                .tenant
                .map(TenantId::new)
                .transpose()
                .map_err(AuthError::Claim)?,
            groups: claims
                .groups
                .map(StringListClaim::into_values)
                .unwrap_or_default()
                .into_iter()
                .map(GroupId::new)
                .collect::<Result<_, _>>()
                .map_err(AuthError::Claim)?,
            // Per-group roles are not asserted in the ID-JAG claim set today;
            // bare membership resolves to Read via Principal::group_memberships().
            group_roles: BTreeSet::new(),
            roles: claims
                .roles
                .map(StringListClaim::into_values)
                .unwrap_or_default()
                .into_iter()
                .map(RoleId::new)
                .collect::<Result<_, _>>()
                .map_err(AuthError::Claim)?,
            scopes: scopes.clone(),
            data_labels: claims
                .data_labels
                .map(StringListClaim::into_values)
                .unwrap_or_default()
                .into_iter()
                .map(DataLabelId::new)
                .collect::<Result<_, _>>()
                .map_err(AuthError::Claim)?,
            assurances: principal_assurances(claims.principal_assurances)?,
            authenticated_at: Some(Utc::now()),
        };

        Ok(VerifiedIdJag {
            client_id,
            principal,
            scopes,
            jwt_id: JwtId::new(claims.jti).map_err(AuthError::Claim)?,
            expires_at: unix_timestamp(claims.exp, "exp")?,
        })
    }
}
