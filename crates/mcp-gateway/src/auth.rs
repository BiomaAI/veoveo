use std::collections::BTreeSet;

mod claims;
mod config;
mod support;

use chrono::{DateTime, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use veoveo_mcp_contract::{
    AccessTokenSubject, DataLabelId, GroupId, JwtId, OAuthClientId, Principal, PrincipalId,
    PrincipalKind, RoleId, ScopeName, TenantId, TokenIssuer, TokenSubject,
};

use claims::{ClientAssertionClaims, IdJagClaims, JwtClaims, OidcIdTokenClaims, StringListClaim};
pub use config::{
    BearerToken, ClientAssertionConfig, IdJagConfig, JwtAuthConfig, OidcIdTokenConfig,
};
pub use support::AuthError;
use support::{allowed_algorithms_for_header, unix_timestamp, validate_jwk_algorithm};

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

        let algorithms = self.allowed_algorithms_for_header(header.alg)?;
        let mut validation = Validation::new(header.alg);
        validation.algorithms = algorithms;
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

    fn allowed_algorithms_for_header(
        &self,
        algorithm: Algorithm,
    ) -> Result<Vec<Algorithm>, AuthError> {
        allowed_algorithms_for_header(&self.config.algorithms, algorithm)
    }
}

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
        let subject = TokenSubject::new(claims.sub.clone()).map_err(AuthError::Claim)?;
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
            authenticated_at: Some(Utc::now()),
        };

        Ok(VerifiedOidcIdentity {
            principal,
            expires_at: unix_timestamp(claims.exp, "exp")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedSubject {
    pub access_token: AccessTokenSubject,
    pub principal: Principal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedClientAssertion {
    pub client_id: OAuthClientId,
    pub jwt_id: JwtId,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdJag {
    pub client_id: OAuthClientId,
    pub principal: Principal,
    pub scopes: BTreeSet<ScopeName>,
    pub jwt_id: JwtId,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedOidcIdentity {
    pub principal: Principal,
    pub expires_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
    use jsonwebtoken::{
        Algorithm, EncodingKey, Header, encode,
        jwk::{Jwk, JwkSet},
    };
    use serde::Serialize;
    use veoveo_mcp_contract::{OidcClientId, OidcNonce, ProtectedResourceId};

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

    #[derive(Debug, Serialize)]
    struct TestClientAssertionClaims<'a> {
        iss: &'a str,
        sub: &'a str,
        aud: &'a str,
        exp: u64,
        nbf: u64,
        iat: u64,
        jti: &'a str,
    }

    #[derive(Debug, Serialize)]
    struct TestIdJagClaims<'a> {
        iss: &'a str,
        sub: &'a str,
        aud: &'a str,
        resource: &'a str,
        client_id: &'a str,
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

    #[derive(Debug, Serialize)]
    struct TestOidcIdTokenClaims<'a> {
        iss: &'a str,
        sub: &'a str,
        aud: &'a str,
        exp: u64,
        nbf: u64,
        iat: u64,
        nonce: &'a str,
        groups: Vec<&'a str>,
        roles: Vec<&'a str>,
        tenant: &'a str,
        data_labels: Vec<&'a str>,
    }

    fn verifier(required_scopes: &[&str]) -> JwtVerifier {
        verifier_with_algorithms(required_scopes, vec![Algorithm::RS256])
    }

    fn verifier_with_algorithms(
        required_scopes: &[&str],
        algorithms: Vec<Algorithm>,
    ) -> JwtVerifier {
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
                algorithms,
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
        BearerToken::from_authorization_header(&format!("Bearer {token}")).expect("token parses")
    }

    fn client_assertion(subject: &str, audience: &str, jwt_id: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".to_string());
        let encoding_key = rsa_encoding_key();
        encode(
            &header,
            &TestClientAssertionClaims {
                iss: "veoveo-headless",
                sub: subject,
                aud: audience,
                exp: 4_102_444_800,
                nbf: 1_700_000_000,
                iat: 1_700_000_000,
                jti: jwt_id,
            },
            &encoding_key,
        )
        .expect("client assertion encodes")
    }

    fn id_jag(resource: &str, jwt_id: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".to_string());
        let encoding_key = rsa_encoding_key();
        encode(
            &header,
            &TestIdJagClaims {
                iss: ISSUER,
                sub: "00u123",
                aud: "https://veoveo.bioma.ai/oauth/default",
                resource,
                client_id: "veoveo-browser",
                exp: 4_102_444_800,
                nbf: 1_700_000_000,
                iat: 1_700_000_000,
                jti: jwt_id,
                scope: "media:use",
                groups: vec!["engineering"],
                roles: vec!["operator"],
                tenant: "tenant-a",
                data_labels: vec!["cui"],
            },
            &encoding_key,
        )
        .expect("ID-JAG encodes")
    }

    fn oidc_id_token(nonce: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".to_string());
        let encoding_key = rsa_encoding_key();
        encode(
            &header,
            &TestOidcIdTokenClaims {
                iss: ISSUER,
                sub: "00u123",
                aud: "veoveo-gateway",
                exp: 4_102_444_800,
                nbf: 1_700_000_000,
                iat: 1_700_000_000,
                nonce,
                groups: vec!["engineering"],
                roles: vec!["operator"],
                tenant: "tenant-a",
                data_labels: vec!["cui"],
            },
            &encoding_key,
        )
        .expect("OIDC ID token encodes")
    }

    fn rsa_encoding_key() -> EncodingKey {
        let der_text = RSA_PRIVATE_KEY_DER_B64.lines().collect::<String>();
        let der = BASE64_STANDARD
            .decode(der_text)
            .expect("base64 RSA test key");
        EncodingKey::from_rsa_der(&der)
    }

    fn jwks() -> JwkSet {
        let encoding_key = rsa_encoding_key();
        let mut jwk =
            Jwk::from_encoding_key(&encoding_key, Algorithm::RS256).expect("jwk from RSA key");
        jwk.common.key_id = Some("test-key".to_string());
        JwkSet { keys: vec![jwk] }
    }

    #[test]
    fn bearer_header_parser_is_strict() {
        assert!(BearerToken::from_authorization_header("Bearer abc.def.ghi").is_ok());
        assert!(BearerToken::from_authorization_header("Basic abc").is_err());
        assert!(BearerToken::from_authorization_header("Bearer").is_err());
        assert!(BearerToken::from_authorization_header("Bearer abc def").is_err());
    }

    #[test]
    fn rejects_symmetric_gateway_jwt_algorithms() {
        let err = JwtAuthConfig::new(
            TokenIssuer::new(ISSUER).unwrap(),
            ProtectedResourceId::new(AUDIENCE).unwrap(),
            Default::default(),
            vec![Algorithm::HS256],
        )
        .expect_err("symmetric algorithms must be rejected");

        assert!(matches!(
            err,
            AuthError::SymmetricAlgorithmNotAllowed(Algorithm::HS256)
        ));
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
    fn verifies_signed_jwt_with_mixed_public_algorithm_policy() {
        let subject = verifier_with_algorithms(
            &["media:use"],
            vec![
                Algorithm::RS256,
                Algorithm::RS384,
                Algorithm::RS512,
                Algorithm::PS256,
                Algorithm::PS384,
                Algorithm::PS512,
                Algorithm::ES256,
                Algorithm::ES384,
                Algorithm::EdDSA,
            ],
        )
        .verify(&token("media:use"))
        .expect("valid token");

        assert_eq!(subject.access_token.subject.as_str(), "00u123");
    }

    #[test]
    fn rejects_missing_required_scope() {
        let err = verifier(&["media:admin"])
            .verify(&token("media:use"))
            .expect_err("scope should be required");

        assert!(matches!(err, AuthError::MissingRequiredScope));
    }

    #[test]
    fn verifies_private_key_jwt_client_assertion() {
        let verifier = ClientAssertionVerifier::new(
            ClientAssertionConfig::new(
                OAuthClientId::new("veoveo-headless").unwrap(),
                "https://veoveo.bioma.ai/oauth/default/token",
                vec![Algorithm::RS256],
            )
            .unwrap(),
            jwks(),
        );

        let assertion = client_assertion(
            "veoveo-headless",
            "https://veoveo.bioma.ai/oauth/default/token",
            "client-jti-1",
        );
        let verified = verifier.verify(&assertion).expect("valid assertion");

        assert_eq!(verified.client_id.as_str(), "veoveo-headless");
        assert_eq!(verified.jwt_id.as_str(), "client-jti-1");
    }

    #[test]
    fn rejects_private_key_jwt_subject_mismatch() {
        let verifier = ClientAssertionVerifier::new(
            ClientAssertionConfig::new(
                OAuthClientId::new("veoveo-headless").unwrap(),
                "https://veoveo.bioma.ai/oauth/default/token",
                vec![Algorithm::RS256],
            )
            .unwrap(),
            jwks(),
        );

        let assertion = client_assertion(
            "other-client",
            "https://veoveo.bioma.ai/oauth/default/token",
            "client-jti-2",
        );
        let err = verifier
            .verify(&assertion)
            .expect_err("subject mismatch should fail");

        assert!(matches!(err, AuthError::ClientAssertionSubjectMismatch));
    }

    #[test]
    fn verifies_enterprise_managed_id_jag() {
        let verifier = IdJagVerifier::new(
            IdJagConfig::new(
                TokenIssuer::new(ISSUER).unwrap(),
                TokenIssuer::new("https://veoveo.bioma.ai/oauth/default").unwrap(),
                ProtectedResourceId::new(AUDIENCE).unwrap(),
                vec![Algorithm::RS256],
            )
            .unwrap(),
            jwks(),
        );

        let verified = verifier
            .verify(&id_jag(AUDIENCE, "id-jag-1"))
            .expect("valid ID-JAG");

        assert_eq!(verified.client_id.as_str(), "veoveo-browser");
        assert_eq!(verified.principal.subject.as_str(), "00u123");
        assert!(
            verified
                .scopes
                .contains(&ScopeName::new("media:use").unwrap())
        );
        assert!(
            verified
                .principal
                .data_labels
                .contains(&DataLabelId::new("cui").unwrap())
        );
    }

    #[test]
    fn rejects_id_jag_for_wrong_resource() {
        let verifier = IdJagVerifier::new(
            IdJagConfig::new(
                TokenIssuer::new(ISSUER).unwrap(),
                TokenIssuer::new("https://veoveo.bioma.ai/oauth/default").unwrap(),
                ProtectedResourceId::new(AUDIENCE).unwrap(),
                vec![Algorithm::RS256],
            )
            .unwrap(),
            jwks(),
        );

        let err = verifier
            .verify(&id_jag("https://veoveo.bioma.ai/mcp/other", "id-jag-2"))
            .expect_err("wrong resource should fail");

        assert!(matches!(err, AuthError::InvalidIdentityAssertionResource));
    }

    #[test]
    fn verifies_oidc_id_token_and_maps_principal() {
        let verifier = OidcIdTokenVerifier::new(
            OidcIdTokenConfig::new(
                TokenIssuer::new(ISSUER).unwrap(),
                OidcClientId::new("veoveo-gateway").unwrap(),
                OidcNonce::new("nonce-1").unwrap(),
                vec![Algorithm::RS256],
            )
            .unwrap(),
            jwks(),
        );

        let verified = verifier
            .verify(&oidc_id_token("nonce-1"))
            .expect("valid OIDC ID token");

        assert_eq!(verified.principal.subject.as_str(), "00u123");
        assert_eq!(
            verified.principal.id.as_str(),
            "https://idp.example.com#00u123"
        );
        assert!(
            verified
                .principal
                .data_labels
                .contains(&DataLabelId::new("cui").unwrap())
        );
    }

    #[test]
    fn rejects_oidc_id_token_nonce_mismatch() {
        let verifier = OidcIdTokenVerifier::new(
            OidcIdTokenConfig::new(
                TokenIssuer::new(ISSUER).unwrap(),
                OidcClientId::new("veoveo-gateway").unwrap(),
                OidcNonce::new("nonce-1").unwrap(),
                vec![Algorithm::RS256],
            )
            .unwrap(),
            jwks(),
        );

        let err = verifier
            .verify(&oidc_id_token("nonce-2"))
            .expect_err("nonce mismatch should fail");

        assert!(matches!(err, AuthError::InvalidOidcNonce));
    }
}
