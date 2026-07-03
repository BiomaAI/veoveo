use std::fmt;

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, jwk::KeyAlgorithm};

#[derive(Debug)]
pub enum AuthError {
    MissingAllowedAlgorithms,
    InvalidAuthorizationHeader,
    InvalidAuthorizationScheme,
    InvalidBearerToken,
    SymmetricAlgorithmNotAllowed(Algorithm),
    MissingKeyId,
    UnknownKeyId(String),
    DisallowedAlgorithm(Algorithm),
    JwkAlgorithmMismatch { token: Algorithm, jwk: KeyAlgorithm },
    MissingRequiredScope,
    InvalidClientAssertion,
    InvalidClientAssertionAudience,
    ClientAssertionSubjectMismatch,
    InvalidIdentityAssertion,
    InvalidOidcIdToken,
    InvalidOidcNonce,
    InvalidPrincipalAssurance(String),
    MissingIdentityAssertionResource,
    InvalidIdentityAssertionResource,
    MissingIdentityAssertionScope,
    InvalidTimestamp { claim: &'static str, value: u64 },
    Claim(veoveo_mcp_contract::IdentifierError),
    Jwt(jsonwebtoken::errors::Error),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAllowedAlgorithms => write!(f, "no JWT algorithms configured"),
            Self::InvalidAuthorizationHeader => write!(f, "invalid Authorization header"),
            Self::InvalidAuthorizationScheme => write!(f, "Authorization scheme must be Bearer"),
            Self::InvalidBearerToken => write!(f, "invalid bearer token"),
            Self::SymmetricAlgorithmNotAllowed(algorithm) => {
                write!(f, "symmetric JWT algorithm `{algorithm:?}` is not allowed")
            }
            Self::MissingKeyId => write!(f, "JWT header is missing kid"),
            Self::UnknownKeyId(key_id) => write!(f, "JWT key id `{key_id}` is not trusted"),
            Self::DisallowedAlgorithm(algorithm) => {
                write!(f, "JWT algorithm `{algorithm:?}` is not allowed")
            }
            Self::JwkAlgorithmMismatch { token, jwk } => write!(
                f,
                "JWT algorithm `{token:?}` does not match trusted JWK algorithm `{jwk:?}`"
            ),
            Self::MissingRequiredScope => write!(f, "JWT is missing required gateway scope"),
            Self::InvalidClientAssertion => write!(f, "invalid client assertion"),
            Self::InvalidClientAssertionAudience => {
                write!(f, "invalid client assertion audience")
            }
            Self::ClientAssertionSubjectMismatch => {
                write!(f, "client assertion subject must match client id")
            }
            Self::InvalidIdentityAssertion => write!(f, "invalid identity assertion"),
            Self::InvalidOidcIdToken => write!(f, "invalid OIDC ID token"),
            Self::InvalidOidcNonce => write!(f, "OIDC ID token nonce does not match request"),
            Self::InvalidPrincipalAssurance(value) => {
                write!(f, "invalid principal assurance `{value}`")
            }
            Self::MissingIdentityAssertionResource => {
                write!(f, "identity assertion is missing resource")
            }
            Self::InvalidIdentityAssertionResource => {
                write!(
                    f,
                    "identity assertion resource does not match gateway profile"
                )
            }
            Self::MissingIdentityAssertionScope => {
                write!(f, "identity assertion is missing scope")
            }
            Self::InvalidTimestamp { claim, value } => {
                write!(f, "JWT claim `{claim}` has invalid timestamp `{value}`")
            }
            Self::Claim(err) => write!(f, "invalid JWT claim: {err}"),
            Self::Jwt(err) => write!(f, "JWT validation failed: {err}"),
        }
    }
}

impl std::error::Error for AuthError {}

pub(super) fn is_symmetric_algorithm(algorithm: Algorithm) -> bool {
    matches!(
        algorithm,
        Algorithm::HS256 | Algorithm::HS384 | Algorithm::HS512
    )
}

pub(super) fn allowed_algorithms_for_header(
    configured_algorithms: &[Algorithm],
    algorithm: Algorithm,
) -> Result<Vec<Algorithm>, AuthError> {
    let algorithms = configured_algorithms
        .iter()
        .copied()
        .filter(|candidate| candidate.family() == algorithm.family())
        .collect::<Vec<_>>();
    if algorithms.is_empty() {
        Err(AuthError::DisallowedAlgorithm(algorithm))
    } else {
        Ok(algorithms)
    }
}

pub(super) fn validate_jwk_algorithm(
    jwk_algorithm: Option<KeyAlgorithm>,
    token_algorithm: Algorithm,
) -> Result<(), AuthError> {
    let Some(jwk_algorithm) = jwk_algorithm else {
        return Ok(());
    };
    let expected = match jwk_algorithm {
        KeyAlgorithm::HS256 => Algorithm::HS256,
        KeyAlgorithm::HS384 => Algorithm::HS384,
        KeyAlgorithm::HS512 => Algorithm::HS512,
        KeyAlgorithm::ES256 => Algorithm::ES256,
        KeyAlgorithm::ES384 => Algorithm::ES384,
        KeyAlgorithm::RS256 => Algorithm::RS256,
        KeyAlgorithm::RS384 => Algorithm::RS384,
        KeyAlgorithm::RS512 => Algorithm::RS512,
        KeyAlgorithm::PS256 => Algorithm::PS256,
        KeyAlgorithm::PS384 => Algorithm::PS384,
        KeyAlgorithm::PS512 => Algorithm::PS512,
        KeyAlgorithm::EdDSA => Algorithm::EdDSA,
        _ => {
            return Err(AuthError::JwkAlgorithmMismatch {
                token: token_algorithm,
                jwk: jwk_algorithm,
            });
        }
    };
    if expected == token_algorithm {
        Ok(())
    } else {
        Err(AuthError::JwkAlgorithmMismatch {
            token: token_algorithm,
            jwk: jwk_algorithm,
        })
    }
}

pub(super) fn unix_timestamp(value: u64, claim: &'static str) -> Result<DateTime<Utc>, AuthError> {
    DateTime::from_timestamp(value as i64, 0).ok_or(AuthError::InvalidTimestamp { claim, value })
}
