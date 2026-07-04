use std::fmt;

use chrono::{DateTime, Utc};
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode,
    errors::Error as JwtError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    GatewayProfileId, IdentifierError, JwtId, Principal, PrincipalId, ServerSlug, TokenIssuer,
};

pub const GATEWAY_INTERNAL_TOKEN_ISSUER: &str = "veoveo-gateway";
pub const MIN_INTERNAL_TOKEN_SECRET_BYTES: usize = 32;

#[derive(Clone, PartialEq, Eq)]
pub struct InternalTokenSecret(String);

impl InternalTokenSecret {
    pub fn new(value: impl Into<String>) -> Result<Self, InternalTokenError> {
        let value = value.into();
        if value.len() < MIN_INTERNAL_TOKEN_SECRET_BYTES {
            return Err(InternalTokenError::SecretTooShort {
                actual: value.len(),
                minimum: MIN_INTERNAL_TOKEN_SECRET_BYTES,
            });
        }
        if value.chars().any(char::is_control) {
            return Err(InternalTokenError::SecretContainsControlCharacter);
        }
        Ok(Self(value))
    }

    fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl fmt::Debug for InternalTokenSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InternalTokenSecret(<redacted>)")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayInternalIdentity {
    pub issuer: TokenIssuer,
    pub profile: GatewayProfileId,
    pub server: ServerSlug,
    pub principal: Principal,
    pub jwt_id: JwtId,
    pub issued_at: DateTime<Utc>,
    pub not_before: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedGatewayInternalToken {
    pub bearer_token: String,
    pub identity: GatewayInternalIdentity,
}

#[derive(Debug, Clone)]
pub struct GatewayInternalTokenIssuer {
    issuer: TokenIssuer,
    secret: InternalTokenSecret,
}

impl GatewayInternalTokenIssuer {
    pub fn new(issuer: TokenIssuer, secret: InternalTokenSecret) -> Self {
        Self { issuer, secret }
    }

    pub fn issue(
        &self,
        profile: GatewayProfileId,
        server: ServerSlug,
        principal: Principal,
        expires_at: DateTime<Utc>,
    ) -> Result<IssuedGatewayInternalToken, InternalTokenError> {
        let now = Utc::now();
        if expires_at <= now {
            return Err(InternalTokenError::ExpiredDelegation);
        }
        let jwt_id =
            JwtId::new(uuid::Uuid::new_v4().to_string()).map_err(InternalTokenError::Identifier)?;
        let identity = GatewayInternalIdentity {
            issuer: self.issuer.clone(),
            profile,
            server,
            principal,
            jwt_id,
            issued_at: now,
            not_before: now,
            expires_at,
        };
        let claims = GatewayInternalJwtClaims::from_identity(&identity);
        let mut header = Header::new(Algorithm::HS256);
        header.typ = Some("JWT".to_string());
        let bearer_token = encode(
            &header,
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(InternalTokenError::Jwt)?;
        Ok(IssuedGatewayInternalToken {
            bearer_token,
            identity,
        })
    }
}

#[derive(Debug, Clone)]
pub struct GatewayInternalTokenVerifier {
    issuer: TokenIssuer,
    audience: ServerSlug,
    secret: InternalTokenSecret,
}

impl GatewayInternalTokenVerifier {
    pub fn new(issuer: TokenIssuer, audience: ServerSlug, secret: InternalTokenSecret) -> Self {
        Self {
            issuer,
            audience,
            secret,
        }
    }

    pub fn verify(
        &self,
        bearer_token: &str,
    ) -> Result<GatewayInternalIdentity, InternalTokenError> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.algorithms = vec![Algorithm::HS256];
        validation.validate_nbf = true;
        validation.leeway = 0;
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub", "iat", "nbf", "jti"]);
        let token = decode::<GatewayInternalJwtClaims>(
            bearer_token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )
        .map_err(InternalTokenError::Jwt)?;
        let claims = token.claims;
        if claims.server != self.audience {
            return Err(InternalTokenError::AudienceMismatch {
                expected: self.audience.clone(),
                actual: claims.server,
            });
        }
        if PrincipalId::new(claims.sub.clone()).map_err(InternalTokenError::Identifier)?
            != claims.principal.id
        {
            return Err(InternalTokenError::SubjectPrincipalMismatch);
        }
        Ok(GatewayInternalIdentity {
            issuer: claims.iss,
            profile: claims.profile,
            server: claims.server,
            principal: claims.principal,
            jwt_id: claims.jti,
            issued_at: timestamp_to_datetime(claims.iat, "iat")?,
            not_before: timestamp_to_datetime(claims.nbf, "nbf")?,
            expires_at: timestamp_to_datetime(claims.exp, "exp")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct GatewayInternalJwtClaims {
    iss: TokenIssuer,
    sub: String,
    aud: String,
    exp: i64,
    nbf: i64,
    iat: i64,
    jti: JwtId,
    profile: GatewayProfileId,
    server: ServerSlug,
    principal: Principal,
}

impl GatewayInternalJwtClaims {
    fn from_identity(identity: &GatewayInternalIdentity) -> Self {
        Self {
            iss: identity.issuer.clone(),
            sub: identity.principal.id.as_str().to_string(),
            aud: identity.server.as_str().to_string(),
            exp: identity.expires_at.timestamp(),
            nbf: identity.not_before.timestamp(),
            iat: identity.issued_at.timestamp(),
            jti: identity.jwt_id.clone(),
            profile: identity.profile.clone(),
            server: identity.server.clone(),
            principal: identity.principal.clone(),
        }
    }
}

#[derive(Debug)]
pub enum InternalTokenError {
    SecretTooShort {
        actual: usize,
        minimum: usize,
    },
    SecretContainsControlCharacter,
    ExpiredDelegation,
    AudienceMismatch {
        expected: ServerSlug,
        actual: ServerSlug,
    },
    SubjectPrincipalMismatch,
    InvalidTimestamp {
        claim: &'static str,
        value: i64,
    },
    Identifier(IdentifierError),
    Jwt(JwtError),
}

impl fmt::Display for InternalTokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SecretTooShort { actual, minimum } => write!(
                f,
                "internal token secret is {actual} byte(s); minimum is {minimum}"
            ),
            Self::SecretContainsControlCharacter => {
                f.write_str("internal token secret must not contain control characters")
            }
            Self::ExpiredDelegation => {
                f.write_str("internal token expiration is not in the future")
            }
            Self::AudienceMismatch { expected, actual } => write!(
                f,
                "internal token audience mismatch: expected `{expected}`, got `{actual}`"
            ),
            Self::SubjectPrincipalMismatch => {
                f.write_str("internal token subject does not match embedded principal")
            }
            Self::InvalidTimestamp { claim, value } => {
                write!(
                    f,
                    "internal token claim `{claim}` has invalid timestamp `{value}`"
                )
            }
            Self::Identifier(err) => write!(f, "invalid internal token identifier: {err}"),
            Self::Jwt(err) => write!(f, "internal token JWT validation failed: {err}"),
        }
    }
}

impl std::error::Error for InternalTokenError {}

fn timestamp_to_datetime(
    value: i64,
    claim: &'static str,
) -> Result<DateTime<Utc>, InternalTokenError> {
    DateTime::from_timestamp(value, 0).ok_or(InternalTokenError::InvalidTimestamp { claim, value })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::TimeDelta;

    use super::*;
    use crate::{GroupId, PrincipalKind, RoleId, ScopeName, TenantId, TokenSubject};

    fn secret() -> InternalTokenSecret {
        InternalTokenSecret::new("local-dev-internal-token-secret-32-bytes-minimum").unwrap()
    }

    fn principal() -> Principal {
        Principal {
            id: PrincipalId::new("https://idp.example.com#user-1").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            subject: TokenSubject::new("user-1").unwrap(),
            tenant: Some(TenantId::new("tenant-a").unwrap()),
            groups: BTreeSet::from([GroupId::new("engineering").unwrap()]),
            roles: BTreeSet::from([RoleId::new("operator").unwrap()]),
            scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: Some(Utc::now()),
        }
    }

    #[test]
    fn rejects_short_internal_token_secret() {
        assert!(matches!(
            InternalTokenSecret::new("too-short"),
            Err(InternalTokenError::SecretTooShort { .. })
        ));
    }

    #[test]
    fn internal_token_round_trips_typed_identity() {
        let issuer =
            GatewayInternalTokenIssuer::new(TokenIssuer::new("veoveo-gateway").unwrap(), secret());
        let issued = issuer
            .issue(
                GatewayProfileId::new("default").unwrap(),
                ServerSlug::new("media").unwrap(),
                principal(),
                Utc::now() + TimeDelta::minutes(5),
            )
            .unwrap();

        let verified = GatewayInternalTokenVerifier::new(
            TokenIssuer::new("veoveo-gateway").unwrap(),
            ServerSlug::new("media").unwrap(),
            secret(),
        )
        .verify(&issued.bearer_token)
        .unwrap();

        assert_eq!(verified.profile.as_str(), "default");
        assert_eq!(verified.server.as_str(), "media");
        assert_eq!(
            verified.principal.id.as_str(),
            "https://idp.example.com#user-1"
        );
    }

    #[test]
    fn internal_token_rejects_wrong_server_audience() {
        let issuer =
            GatewayInternalTokenIssuer::new(TokenIssuer::new("veoveo-gateway").unwrap(), secret());
        let issued = issuer
            .issue(
                GatewayProfileId::new("default").unwrap(),
                ServerSlug::new("media").unwrap(),
                principal(),
                Utc::now() + TimeDelta::minutes(5),
            )
            .unwrap();

        let err = GatewayInternalTokenVerifier::new(
            TokenIssuer::new("veoveo-gateway").unwrap(),
            ServerSlug::new("simulation").unwrap(),
            secret(),
        )
        .verify(&issued.bearer_token)
        .expect_err("wrong server audience should be rejected");

        assert!(matches!(err, InternalTokenError::Jwt(_)));
    }
}
