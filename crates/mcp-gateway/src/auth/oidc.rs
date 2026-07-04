use std::collections::BTreeSet;

use chrono::Utc;
use jsonwebtoken::{DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use veoveo_mcp_contract::{
    DataLabelId, GroupId, IdentityProviderSubjectClaim, IdentityProviderTenantClaim,
    IdentityProviderTenantClaimMapping, Principal, PrincipalId, PrincipalKind, RoleId, TenantId,
    TokenIssuer, TokenSubject,
};

use super::{
    claims::{OidcIdTokenClaims, StringListClaim},
    config::OidcIdTokenConfig,
    principal::principal_assurances,
    support::{AuthError, allowed_algorithms_for_header, unix_timestamp, validate_jwk_algorithm},
    verified::VerifiedOidcIdentity,
};

#[derive(Debug, Clone)]
pub struct OidcIdTokenVerifier {
    config: OidcIdTokenConfig,
    jwks: JwkSet,
}

impl OidcIdTokenVerifier {
    pub fn new(config: OidcIdTokenConfig, jwks: JwkSet) -> Self {
        Self { config, jwks }
    }

    pub fn verify(&self, id_token: &str) -> Result<VerifiedOidcIdentity, AuthError> {
        if id_token.is_empty() || id_token.chars().any(char::is_whitespace) {
            return Err(AuthError::InvalidOidcIdToken);
        }
        let header = decode_header(id_token).map_err(AuthError::Jwt)?;
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
        validation.set_audience(&[self.config.client_id.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "iat", "nonce"]);

        let data =
            decode::<OidcIdTokenClaims>(id_token, &key, &validation).map_err(AuthError::Jwt)?;
        let claims = data.claims;
        if claims.nonce.as_deref() != Some(self.config.nonce.as_str()) {
            return Err(AuthError::InvalidOidcNonce);
        }

        let issuer = TokenIssuer::new(claims.iss.clone()).map_err(AuthError::Claim)?;
        let subject = TokenSubject::new(
            subject_claim(&claims, self.config.claim_mapping.subject)?.to_owned(),
        )
        .map_err(AuthError::Claim)?;
        let principal = Principal {
            id: PrincipalId::new(format!("{issuer}#{subject}")).map_err(AuthError::Claim)?,
            kind: PrincipalKind::User,
            issuer,
            subject,
            tenant: tenant_claim(&claims, self.config.claim_mapping.tenant.as_ref())?
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
            roles: claims
                .roles
                .map(StringListClaim::into_values)
                .unwrap_or_default()
                .into_iter()
                .map(RoleId::new)
                .collect::<Result<_, _>>()
                .map_err(AuthError::Claim)?,
            scopes: BTreeSet::new(),
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

        Ok(VerifiedOidcIdentity {
            principal,
            expires_at: unix_timestamp(claims.exp, "exp")?,
        })
    }
}

fn subject_claim(
    claims: &OidcIdTokenClaims,
    mapping: IdentityProviderSubjectClaim,
) -> Result<&str, AuthError> {
    let value = match mapping {
        IdentityProviderSubjectClaim::Sub => Some(claims.sub.as_str()),
        IdentityProviderSubjectClaim::Oid => claims.oid.as_deref(),
        IdentityProviderSubjectClaim::Email => claims.email.as_deref(),
        IdentityProviderSubjectClaim::PreferredUsername => claims.preferred_username.as_deref(),
    }
    .map(str::trim)
    .filter(|value| !value.is_empty());

    value.ok_or(match mapping {
        IdentityProviderSubjectClaim::Sub => AuthError::MissingMappedClaim("sub"),
        IdentityProviderSubjectClaim::Oid => AuthError::MissingMappedClaim("oid"),
        IdentityProviderSubjectClaim::Email => AuthError::MissingMappedClaim("email"),
        IdentityProviderSubjectClaim::PreferredUsername => {
            AuthError::MissingMappedClaim("preferred_username")
        }
    })
}

fn tenant_claim(
    claims: &OidcIdTokenClaims,
    mapping: Option<&IdentityProviderTenantClaimMapping>,
) -> Result<Option<String>, AuthError> {
    let Some(mapping) = mapping else {
        return Ok(None);
    };
    let raw = match mapping.claim {
        IdentityProviderTenantClaim::Tenant => claims.tenant.as_deref(),
        IdentityProviderTenantClaim::Tid => claims.tid.as_deref(),
    }
    .map(str::trim)
    .filter(|value| !value.is_empty());

    let Some(raw) = raw else {
        return Ok(None);
    };
    if mapping.values.is_empty() {
        return Ok(Some(raw.to_owned()));
    }
    mapping
        .values
        .get(raw)
        .map(|tenant| Some(tenant.to_string()))
        .ok_or_else(|| AuthError::UnmappedTenantClaim(raw.to_owned()))
}
