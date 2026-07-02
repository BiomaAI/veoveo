use std::{collections::BTreeSet, fmt};

use chrono::{DateTime, Utc};
use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{JwkSet, KeyAlgorithm},
};
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    AccessTokenSubject, DataLabelId, GroupId, JwtId, Principal, PrincipalId, PrincipalKind,
    ProtectedResourceId, RoleId, ScopeName, TenantId, TokenIssuer, TokenSubject,
};

#[derive(Debug, Clone)]
pub struct JwtAuthConfig {
    pub issuer: TokenIssuer,
    pub audience: ProtectedResourceId,
    pub required_scopes: BTreeSet<ScopeName>,
    pub algorithms: Vec<Algorithm>,
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
        Ok(Self {
            issuer,
            audience,
            required_scopes,
            algorithms,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BearerToken(String);

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

#[derive(Debug, Clone)]
pub struct JwtVerifier {
    config: JwtAuthConfig,
    jwks: JwkSet,
}

impl JwtVerifier {
    pub fn new(config: JwtAuthConfig, jwks: JwkSet) -> Self {
        Self { config, jwks }
    }

    pub fn verify(&self, token: &BearerToken) -> Result<AuthenticatedSubject, AuthError> {
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

        let mut validation = Validation::new(self.config.algorithms[0]);
        validation.algorithms = self.config.algorithms.clone();
        validation.validate_nbf = true;
        validation.set_issuer(&[self.config.issuer.as_str()]);
        validation.set_audience(&[self.config.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);

        let data =
            decode::<JwtClaims>(token.as_str(), &key, &validation).map_err(AuthError::Jwt)?;
        let claims = data.claims;
        let scopes = claims.scopes()?;
        if !self.config.required_scopes.is_subset(&scopes) {
            return Err(AuthError::MissingRequiredScope);
        }

        let issuer = TokenIssuer::new(claims.iss.clone()).map_err(AuthError::Claim)?;
        let subject = TokenSubject::new(claims.sub.clone()).map_err(AuthError::Claim)?;
        let token_subject = AccessTokenSubject {
            issuer: issuer.clone(),
            subject: subject.clone(),
            audience: self.config.audience.clone(),
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
            id: PrincipalId::new(format!("{issuer}#{subject}")).map_err(AuthError::Claim)?,
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
            authenticated_at: Some(Utc::now()),
        };

        Ok(AuthenticatedSubject {
            access_token: token_subject,
            principal,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSubject {
    pub access_token: AccessTokenSubject,
    pub principal: Principal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct JwtClaims {
    iss: String,
    sub: String,
    aud: StringListClaim,
    exp: u64,
    #[serde(default)]
    nbf: Option<u64>,
    #[serde(default)]
    iat: Option<u64>,
    #[serde(default)]
    jti: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    scp: Option<StringListClaim>,
    #[serde(default)]
    groups: Option<StringListClaim>,
    #[serde(default)]
    roles: Option<StringListClaim>,
    #[serde(default)]
    tenant: Option<String>,
    #[serde(default)]
    data_labels: Option<StringListClaim>,
    #[serde(default)]
    principal_kind: Option<PrincipalKind>,
}

impl JwtClaims {
    fn scopes(&self) -> Result<BTreeSet<ScopeName>, AuthError> {
        let mut values = BTreeSet::new();
        if let Some(scope) = &self.scope {
            for item in scope.split_whitespace() {
                values.insert(ScopeName::new(item).map_err(AuthError::Claim)?);
            }
        }
        if let Some(scp) = &self.scp {
            for item in scp.values() {
                values.insert(ScopeName::new(item).map_err(AuthError::Claim)?);
            }
        }
        Ok(values)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
enum StringListClaim {
    One(String),
    Many(Vec<String>),
}

impl StringListClaim {
    fn values(&self) -> Vec<&str> {
        match self {
            Self::One(value) => vec![value.as_str()],
            Self::Many(values) => values.iter().map(String::as_str).collect(),
        }
    }

    fn into_values(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

#[derive(Debug)]
pub enum AuthError {
    MissingAllowedAlgorithms,
    InvalidAuthorizationHeader,
    InvalidAuthorizationScheme,
    InvalidBearerToken,
    MissingKeyId,
    UnknownKeyId(String),
    DisallowedAlgorithm(Algorithm),
    JwkAlgorithmMismatch { token: Algorithm, jwk: KeyAlgorithm },
    MissingRequiredScope,
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
            Self::InvalidTimestamp { claim, value } => {
                write!(f, "JWT claim `{claim}` has invalid timestamp `{value}`")
            }
            Self::Claim(err) => write!(f, "invalid JWT claim: {err}"),
            Self::Jwt(err) => write!(f, "JWT validation failed: {err}"),
        }
    }
}

impl std::error::Error for AuthError {}

fn validate_jwk_algorithm(
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

fn unix_timestamp(value: u64, claim: &'static str) -> Result<DateTime<Utc>, AuthError> {
    DateTime::from_timestamp(value as i64, 0).ok_or(AuthError::InvalidTimestamp { claim, value })
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
    use jsonwebtoken::{
        Algorithm, EncodingKey, Header, encode,
        jwk::{Jwk, JwkSet},
    };
    use serde::Serialize;

    use super::*;

    const ISSUER: &str = "https://idp.example.com";
    const AUDIENCE: &str = "https://veoveo.bioma.ai/mcp/default";
    const RSA_PRIVATE_KEY_DER_B64: &str = r#"
MIIEpAIBAAKCAQEAvCUS6tGS9/VE3pGzncb1rDsZt/V/LkPHl2QO9jDlaO/jAEdfPOtCSsSyv7dY
+nmY61GpXedIpqg6U7gcU/TcOVar0APPbKZ3OERrvrX9w5/oTJyqK42Lwybl9vmFApcRDIexmSQ8
HBdc1tQPqdkSCHS2csfZVxAQ64PLh48017Q+w8L1UuXYOxD8QdpQx2R1TD3bOiSeaZRs2Utww6rb
ex0/Gn6kkYJw3kr+rQgqmmmOoZuEi7p3qSg6KXvKf3hcfugKQlRIamdP8FOz/3sM2vf2jzUV9BUM
xtOF/yj2GzLmUYHxPtn+K46QDTcGpFyYN6gAPaiGBKkxxZDIaHgosQIDAQABAoIBAAl/bB7tRTht
+ePr8ker2m1PPvc/xgOzgX0BnLU+JuiXGowiLjs8q5graZQeyPe9AXSYpt6CDVN3cNlW1RxCY0ck
OlBqDtOu7BwLrS4/kO/KD9+lNXx1HOn1Odzvv/CPaHmL1JH057Fp1wKTyjYiaoQBg0/USaMY4SfI
e5LsbmgYn71s03MXf9/TgKErBRXiIYPW9aKvpKlfCQ8pGV1/i/rTy+Sj87rk+8+fU+fPVyKUWsjA
gNHm+FmhCPPPVm4qh6Vw/NmuOpfRf1mzfVi7rBq0t5ehHkmW3KVSWY9+v3EttoXjC9iXFIr1OXp5
aoaZZIXpjw3vAlaKwXbuu7lUZhkCgYEA3PGDT2UgWCFjEJjpi2fQzCBfVQC3lgJ8Xwz3EOeNhe+M
mrKb358iDp5o+WgU+S4HJJcGK9uptGgN9GYrf303GPMwmWOvC8xH5fV8WDBYGqMeEi+xFHlS8ymt
MmiWpAkW8/rEjDJama58qzjyEcq+fuW4BJcxOydFHgACSOZIbVkCgYEA2f9RJ7+tOajthShh6LbV
lhSNDjAeauBj5pcg8bZhLaCNWKCUBE2ob+YXvTL6mzx30faY5nutMdJfOI2Au7YqQgx8HeCBkCUi
D5Ngx9yjQ2/vnNQSRjIY2mjj0/tzTlVNGJDxbwUr8DGug8BD6Wz+L1l+s8F3aqAFljp7HLMq8xkC
gYEAsoobgSoH9A+uvPfEKdnPmVRDlS4KLJd/p1OTxz5GV8gXB99zJEa0v7l0vK5F3II8VW4RF5nf
TiCTvj5dwh0OTAQg7qLmDhOauhIg1Cbk20mbADk30IKl7EduZQCtUorh2HB5KY17NxsQNVDEFGqQ
e3zoshT3PITkTnTVY9FrD6kCgYEAwZa5JBpUo6q/Wwu0fuu2mvOfG+VhbbndHY5CBETY4aL9QqI/
L98i4FQt6qeV4zt8kGlz+OIFuQO/6cHHe2rW9haONh4EENTY/Yn8XSAzoBSMbfHqVInyhiq1f6+C
AyM/NryomtW14jTMbFXWOTnANJ4+JTV+baKzs2g1ohP95SkCgYB7RzFmdbiY1ASdGO/vWqc/wLnT
hHID7qgdXU4DP84HMmOX/QG5iV8GtQPTfNJm+m1PEnkg4W24DOqg2gJ3/q7wTROOLwQlJtOmizkC
XVKygdRdax3xMB3Eld5rlIDwzX09ARHrm8badXtrF0NhQPYZVbax8rpJGcgEFPgXEJJ71w==
"#;

    #[derive(Debug, Serialize)]
    struct TestClaims<'a> {
        iss: &'a str,
        sub: &'a str,
        aud: &'a str,
        exp: u64,
        nbf: u64,
        iat: u64,
        jti: &'a str,
        scope: &'a str,
        groups: Vec<&'a str>,
        roles: Vec<&'a str>,
        tenant: &'a str,
        data_labels: Vec<&'a str>,
    }

    fn verifier(required_scopes: &[&str]) -> JwtVerifier {
        let encoding_key = rsa_encoding_key();
        let mut jwk =
            Jwk::from_encoding_key(&encoding_key, Algorithm::RS256).expect("jwk from RSA key");
        jwk.common.key_id = Some("test-key".to_string());
        JwtVerifier::new(
            JwtAuthConfig::new(
                TokenIssuer::new(ISSUER).unwrap(),
                ProtectedResourceId::new(AUDIENCE).unwrap(),
                required_scopes
                    .iter()
                    .map(|scope| ScopeName::new(*scope).unwrap())
                    .collect(),
                vec![Algorithm::RS256],
            )
            .unwrap(),
            JwkSet { keys: vec![jwk] },
        )
    }

    fn token(scope: &str) -> BearerToken {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".to_string());
        let encoding_key = rsa_encoding_key();
        let token = encode(
            &header,
            &TestClaims {
                iss: ISSUER,
                sub: "00u123",
                aud: AUDIENCE,
                exp: 4_102_444_800,
                nbf: 1_700_000_000,
                iat: 1_700_000_000,
                jti: "jwt-1",
                scope,
                groups: vec!["engineering"],
                roles: vec!["operator"],
                tenant: "tenant-a",
                data_labels: vec!["pii", "cui"],
            },
            &encoding_key,
        )
        .expect("token encodes");
        BearerToken(token)
    }

    fn rsa_encoding_key() -> EncodingKey {
        let der_text = RSA_PRIVATE_KEY_DER_B64.lines().collect::<String>();
        let der = BASE64_STANDARD
            .decode(der_text)
            .expect("base64 RSA test key");
        EncodingKey::from_rsa_der(&der)
    }

    #[test]
    fn bearer_header_parser_is_strict() {
        assert!(BearerToken::from_authorization_header("Bearer abc.def.ghi").is_ok());
        assert!(BearerToken::from_authorization_header("Basic abc").is_err());
        assert!(BearerToken::from_authorization_header("Bearer").is_err());
        assert!(BearerToken::from_authorization_header("Bearer abc def").is_err());
    }

    #[test]
    fn verifies_signed_jwt_and_maps_principal() {
        let subject = verifier(&["media:use"])
            .verify(&token("media:use media:read"))
            .expect("valid token");

        assert_eq!(subject.access_token.subject.as_str(), "00u123");
        assert_eq!(subject.access_token.audience.as_str(), AUDIENCE);
        assert_eq!(
            subject.principal.id.as_str(),
            "https://idp.example.com#00u123"
        );
        assert_eq!(subject.principal.tenant.unwrap().as_str(), "tenant-a");
        assert!(
            subject
                .principal
                .scopes
                .contains(&ScopeName::new("media:use").unwrap())
        );
        assert!(
            subject
                .principal
                .data_labels
                .contains(&DataLabelId::new("cui").unwrap())
        );
    }

    #[test]
    fn rejects_missing_required_scope() {
        let err = verifier(&["media:admin"])
            .verify(&token("media:use"))
            .expect_err("scope should be required");

        assert!(matches!(err, AuthError::MissingRequiredScope));
    }
}
