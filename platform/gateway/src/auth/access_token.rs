use std::collections::BTreeSet;

use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use veoveo_mcp_contract::{
    AccessTokenSubject, DataLabelId, DelegationId, GroupId, JwtId, OAuthClientId, Principal,
    PrincipalId, PrincipalKind, RoleId, TenantId, TokenIssuer, TokenSubject, WorkContextId,
};

use super::{
    claims::{JwtClaims, StringListClaim},
    config::{BearerToken, JwtAuthConfig},
    principal::principal_assurances,
    support::{
        AuthError, allowed_algorithms_for_header, ensure_jwt_crypto_provider, unix_timestamp,
        validate_jwk_algorithm,
    },
    verified::VerifiedAccessToken,
};

#[derive(Debug, Clone)]
pub struct JwtVerifier {
    config: JwtAuthConfig,
    jwks: JwkSet,
}

impl JwtVerifier {
    pub fn new(config: JwtAuthConfig, jwks: JwkSet) -> Self {
        Self { config, jwks }
    }

    pub fn verify(&self, token: &BearerToken) -> Result<VerifiedAccessToken, AuthError> {
        ensure_jwt_crypto_provider();
        let header = decode_header(token.as_str()).map_err(AuthError::Jwt)?;
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

        let algorithms = self.allowed_algorithms_for_header(header.alg)?;
        let mut validation = Validation::new(header.alg);
        validation.algorithms = algorithms;
        validation.validate_nbf = true;
        validation.set_issuer(&[self.config.issuer.as_str()]);
        validation.set_audience(&[self.config.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "client_id"]);

        let data =
            decode::<JwtClaims>(token.as_str(), &key, &validation).map_err(AuthError::Jwt)?;
        let claims = data.claims;
        let scopes = claims.scopes()?;
        if !self.config.required_scopes.is_subset(&scopes) {
            return Err(AuthError::MissingRequiredScope);
        }

        let issuer = TokenIssuer::new(claims.iss.clone()).map_err(AuthError::Claim)?;
        let subject = TokenSubject::new(claims.sub.clone()).map_err(AuthError::Claim)?;
        let oauth_client_id =
            OAuthClientId::new(claims.client_id.clone()).map_err(AuthError::Claim)?;
        let token_subject = AccessTokenSubject {
            issuer: issuer.clone(),
            subject: subject.clone(),
            oauth_client_id,
            audience: self.config.audience.clone(),
            work_context: WorkContextId::new(claims.work_context.clone())
                .map_err(AuthError::Claim)?,
            invocation_mode: claims.invocation_mode,
            initiator: claims
                .initiator
                .map(PrincipalId::new)
                .transpose()
                .map_err(AuthError::Claim)?,
            delegation_id: claims
                .delegation_id
                .map(DelegationId::new)
                .transpose()
                .map_err(AuthError::Claim)?,
            scopes: scopes.clone(),
            jwt_id: claims
                .jti
                .map(JwtId::new)
                .transpose()
                .map_err(AuthError::Claim)?,
            issued_at: unix_timestamp(claims.iat.unwrap_or(0), "iat")?,
            not_before: claims
                .nbf
                .map(|value| unix_timestamp(value, "nbf"))
                .transpose()?,
            expires_at: unix_timestamp(claims.exp, "exp")?,
        };
        let principal = Principal {
            id: PrincipalId::new(claims.principal_id).map_err(AuthError::Claim)?,
            kind: claims.principal_kind.unwrap_or(PrincipalKind::User),
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
            // Per-group roles are not asserted by the OAuth access token today;
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
            scopes,
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

        Ok(VerifiedAccessToken {
            access_token: token_subject,
            principal,
        })
    }

    fn allowed_algorithms_for_header(
        &self,
        algorithm: Algorithm,
    ) -> Result<Vec<Algorithm>, AuthError> {
        allowed_algorithms_for_header(&self.config.algorithms, algorithm)
    }
}
