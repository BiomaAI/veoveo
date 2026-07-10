use std::collections::BTreeSet;

use jsonwebtoken::Algorithm;
use veoveo_mcp_contract::{
    IdentityProviderClaimMapping, OAuthClientId, OidcClientId, OidcNonce, ProtectedResourceId,
    ScopeName, TokenIssuer,
};

use super::support::{AuthError, is_symmetric_algorithm};

#[derive(Debug, Clone)]
pub struct JwtAuthConfig {
    pub issuer: TokenIssuer,
    pub audience: ProtectedResourceId,
    pub required_scopes: BTreeSet<ScopeName>,
    pub algorithms: Vec<Algorithm>,
}

#[derive(Debug, Clone)]
pub struct ClientAssertionConfig {
    pub client_id: OAuthClientId,
    pub audience: String,
    pub algorithms: Vec<Algorithm>,
}

#[derive(Debug, Clone)]
pub struct IdJagConfig {
    pub issuer: TokenIssuer,
    pub audience: TokenIssuer,
    pub resource: ProtectedResourceId,
    pub algorithms: Vec<Algorithm>,
}

#[derive(Debug, Clone)]
pub struct OidcIdTokenConfig {
    pub issuer: TokenIssuer,
    pub client_id: OidcClientId,
    pub nonce: OidcNonce,
    pub algorithms: Vec<Algorithm>,
    pub claim_mapping: IdentityProviderClaimMapping,
}

impl ClientAssertionConfig {
    pub fn new(
        client_id: OAuthClientId,
        audience: impl Into<String>,
        algorithms: Vec<Algorithm>,
    ) -> Result<Self, AuthError> {
        if algorithms.is_empty() {
            return Err(AuthError::MissingAllowedAlgorithms);
        }
        if let Some(algorithm) = algorithms
            .iter()
            .copied()
            .find(|algorithm| is_symmetric_algorithm(*algorithm))
        {
            return Err(AuthError::SymmetricAlgorithmNotAllowed(algorithm));
        }
        let audience = audience.into();
        if audience.is_empty() {
            return Err(AuthError::InvalidClientAssertionAudience);
        }
        Ok(Self {
            client_id,
            audience,
            algorithms,
        })
    }
}

impl JwtAuthConfig {
    pub fn new(
        issuer: TokenIssuer,
        audience: ProtectedResourceId,
        required_scopes: BTreeSet<ScopeName>,
        algorithms: Vec<Algorithm>,
    ) -> Result<Self, AuthError> {
        if algorithms.is_empty() {
            return Err(AuthError::MissingAllowedAlgorithms);
        }
        if let Some(algorithm) = algorithms
            .iter()
            .copied()
            .find(|algorithm| is_symmetric_algorithm(*algorithm))
        {
            return Err(AuthError::SymmetricAlgorithmNotAllowed(algorithm));
        }
        Ok(Self {
            issuer,
            audience,
            required_scopes,
            algorithms,
        })
    }
}

impl IdJagConfig {
    pub fn new(
        issuer: TokenIssuer,
        audience: TokenIssuer,
        resource: ProtectedResourceId,
        algorithms: Vec<Algorithm>,
    ) -> Result<Self, AuthError> {
        if algorithms.is_empty() {
            return Err(AuthError::MissingAllowedAlgorithms);
        }
        if let Some(algorithm) = algorithms
            .iter()
            .copied()
            .find(|algorithm| is_symmetric_algorithm(*algorithm))
        {
            return Err(AuthError::SymmetricAlgorithmNotAllowed(algorithm));
        }
        Ok(Self {
            issuer,
            audience,
            resource,
            algorithms,
        })
    }
}

impl OidcIdTokenConfig {
    pub fn new(
        issuer: TokenIssuer,
        client_id: OidcClientId,
        nonce: OidcNonce,
        algorithms: Vec<Algorithm>,
    ) -> Result<Self, AuthError> {
        Self::new_with_claim_mapping(
            issuer,
            client_id,
            nonce,
            algorithms,
            IdentityProviderClaimMapping::default(),
        )
    }

    pub fn new_with_claim_mapping(
        issuer: TokenIssuer,
        client_id: OidcClientId,
        nonce: OidcNonce,
        algorithms: Vec<Algorithm>,
        claim_mapping: IdentityProviderClaimMapping,
    ) -> Result<Self, AuthError> {
        if algorithms.is_empty() {
            return Err(AuthError::MissingAllowedAlgorithms);
        }
        if let Some(algorithm) = algorithms
            .iter()
            .copied()
            .find(|algorithm| is_symmetric_algorithm(*algorithm))
        {
            return Err(AuthError::SymmetricAlgorithmNotAllowed(algorithm));
        }
        Ok(Self {
            issuer,
            client_id,
            nonce,
            algorithms,
            claim_mapping,
        })
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct BearerToken(String);

impl std::fmt::Debug for BearerToken {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("BearerToken([REDACTED])")
    }
}

impl BearerToken {
    pub fn from_authorization_header(value: &str) -> Result<Self, AuthError> {
        let Some((scheme, token)) = value.split_once(' ') else {
            return Err(AuthError::InvalidAuthorizationHeader);
        };
        if !scheme.eq_ignore_ascii_case("bearer") {
            return Err(AuthError::InvalidAuthorizationScheme);
        }
        if token.is_empty() || token.chars().any(char::is_whitespace) {
            return Err(AuthError::InvalidBearerToken);
        }
        Ok(Self(token.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}
