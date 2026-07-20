mod access_token;
mod claims;
mod client_assertion;
mod config;
mod id_jag;
mod oidc;
mod principal;
mod support;
mod verified;

pub use access_token::JwtVerifier;
pub use client_assertion::ClientAssertionVerifier;
pub use config::{
    BearerToken, ClientAssertionConfig, IdJagConfig, JwtAuthConfig, OidcIdTokenConfig,
};
pub use id_jag::IdJagVerifier;
pub use oidc::OidcIdTokenVerifier;
pub use support::AuthError;
pub use verified::{
    AuthenticatedSubject, VerifiedAccessToken, VerifiedClientAssertion, VerifiedIdJag,
    VerifiedOidcIdentity,
};

#[cfg(test)]
mod tests {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
    use jsonwebtoken::{
        Algorithm, EncodingKey, Header, encode,
        jwk::{Jwk, JwkSet},
    };
    use serde::Serialize;
    use veoveo_mcp_contract::{
        DataLabelId, InvocationMode, OAuthClientId, OidcClientId, OidcNonce, PrincipalAssurance,
        ProtectedResourceId, ScopeName, TokenIssuer,
    };

    use super::*;

    const ISSUER: &str = "https://idp.example.com";
    const AUDIENCE: &str = "https://veoveo.example/mcp/operator";
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
        principal_id: &'a str,
        client_id: &'a str,
        work_context: &'a str,
        invocation_mode: InvocationMode,
        initiator: &'a str,
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
        principal_assurances: Vec<&'a str>,
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
        principal_assurances: Vec<&'a str>,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        tenant: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tid: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        oid: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        preferred_username: Option<&'a str>,
        data_labels: Vec<&'a str>,
        principal_assurances: Vec<&'a str>,
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
        token_with_assurances(scope, vec!["us_person"])
    }

    fn token_with_assurances(scope: &str, principal_assurances: Vec<&str>) -> BearerToken {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".to_string());
        let encoding_key = rsa_encoding_key();
        let token = encode(
            &header,
            &TestClaims {
                iss: ISSUER,
                sub: "00u123",
                principal_id: "https://idp.example.com#00u123",
                client_id: "operator-local-public",
                work_context: "mission",
                invocation_mode: InvocationMode::Direct,
                initiator: "https://idp.example.com#00u123",
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
                principal_assurances,
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
                iss: "operator-service",
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
                aud: "https://veoveo.example/oauth",
                resource,
                client_id: "operator-local-public",
                exp: 4_102_444_800,
                nbf: 1_700_000_000,
                iat: 1_700_000_000,
                jti: jwt_id,
                scope: "operator:use",
                groups: vec!["engineering"],
                roles: vec!["operator"],
                tenant: "tenant-a",
                data_labels: vec!["cui"],
                principal_assurances: vec!["us_person"],
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
                aud: "veoveo",
                exp: 4_102_444_800,
                nbf: 1_700_000_000,
                iat: 1_700_000_000,
                nonce,
                groups: vec!["engineering"],
                roles: vec!["operator"],
                tenant: Some("tenant-a"),
                tid: None,
                oid: None,
                email: None,
                preferred_username: None,
                data_labels: vec!["cui"],
                principal_assurances: vec!["us_person"],
            },
            &encoding_key,
        )
        .expect("OIDC ID token encodes")
    }

    fn oidc_entra_id_token(nonce: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".to_string());
        let encoding_key = rsa_encoding_key();
        encode(
            &header,
            &TestOidcIdTokenClaims {
                iss: ISSUER,
                sub: "pairwise-subject",
                aud: "veoveo",
                exp: 4_102_444_800,
                nbf: 1_700_000_000,
                iat: 1_700_000_000,
                nonce,
                groups: vec![],
                roles: vec!["veoveo_operator"],
                tenant: None,
                tid: Some("tenant-a"),
                oid: Some("entra-object-id"),
                email: None,
                preferred_username: None,
                data_labels: vec![],
                principal_assurances: vec![],
            },
            &encoding_key,
        )
        .expect("Entra OIDC ID token encodes")
    }

    fn rsa_encoding_key() -> EncodingKey {
        support::ensure_jwt_crypto_provider();
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
        let token = BearerToken::from_authorization_header("Bearer abc.def.ghi").unwrap();
        assert_eq!(format!("{token:?}"), "BearerToken([REDACTED])");
        assert!(!format!("{token:?}").contains("abc.def.ghi"));
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
        let subject = verifier(&["operator:use"])
            .verify(&token("operator:use media:read"))
            .expect("valid token");

        assert_eq!(subject.access_token.subject.as_str(), "00u123");
        assert_eq!(subject.access_token.audience.as_str(), AUDIENCE);
        assert_eq!(
            subject.access_token.oauth_client_id.as_str(),
            "operator-local-public"
        );
        assert_eq!(
            subject.principal.id.as_str(),
            "https://idp.example.com#00u123"
        );
        assert_eq!(subject.principal.tenant.unwrap().as_str(), "tenant-a");
        assert!(
            subject
                .principal
                .scopes
                .contains(&ScopeName::new("operator:use").unwrap())
        );
        assert!(
            subject
                .principal
                .data_labels
                .contains(&DataLabelId::new("cui").unwrap())
        );
        assert!(
            subject
                .principal
                .assurances
                .contains(&PrincipalAssurance::UsPerson)
        );
    }

    #[test]
    fn verifies_signed_jwt_with_mixed_public_algorithm_policy() {
        let subject = verifier_with_algorithms(
            &["operator:use"],
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
        .verify(&token("operator:use"))
        .expect("valid token");

        assert_eq!(subject.access_token.subject.as_str(), "00u123");
    }

    #[test]
    fn rejects_missing_required_scope() {
        let err = verifier(&["media:admin"])
            .verify(&token("operator:use"))
            .expect_err("scope should be required");

        assert!(matches!(err, AuthError::MissingRequiredScope));
    }

    #[test]
    fn rejects_unknown_principal_assurance_claim() {
        let err = verifier(&["operator:use"])
            .verify(&token_with_assurances("operator:use", vec!["contractor"]))
            .expect_err("unknown assurance should fail closed");

        assert!(
            matches!(err, AuthError::InvalidPrincipalAssurance(value) if value == "contractor")
        );
    }

    #[test]
    fn verifies_private_key_jwt_client_assertion() {
        let verifier = ClientAssertionVerifier::new(
            ClientAssertionConfig::new(
                OAuthClientId::new("operator-service").unwrap(),
                "https://veoveo.example/oauth/token",
                vec![Algorithm::RS256],
            )
            .unwrap(),
            jwks(),
        );

        let assertion = client_assertion(
            "operator-service",
            "https://veoveo.example/oauth/token",
            "client-jti-1",
        );
        let verified = verifier.verify(&assertion).expect("valid assertion");

        assert_eq!(verified.client_id.as_str(), "operator-service");
        assert_eq!(verified.jwt_id.as_str(), "client-jti-1");
    }

    #[test]
    fn rejects_private_key_jwt_subject_mismatch() {
        let verifier = ClientAssertionVerifier::new(
            ClientAssertionConfig::new(
                OAuthClientId::new("operator-service").unwrap(),
                "https://veoveo.example/oauth/token",
                vec![Algorithm::RS256],
            )
            .unwrap(),
            jwks(),
        );

        let assertion = client_assertion(
            "other-client",
            "https://veoveo.example/oauth/token",
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
                TokenIssuer::new("https://veoveo.example/oauth").unwrap(),
                ProtectedResourceId::new(AUDIENCE).unwrap(),
                vec![Algorithm::RS256],
            )
            .unwrap(),
            jwks(),
        );

        let verified = verifier
            .verify(&id_jag(AUDIENCE, "id-jag-1"))
            .expect("valid ID-JAG");

        assert_eq!(verified.client_id.as_str(), "operator-local-public");
        assert_eq!(verified.principal.subject.as_str(), "00u123");
        assert!(
            verified
                .scopes
                .contains(&ScopeName::new("operator:use").unwrap())
        );
        assert!(
            verified
                .principal
                .data_labels
                .contains(&DataLabelId::new("cui").unwrap())
        );
        assert!(
            verified
                .principal
                .assurances
                .contains(&PrincipalAssurance::UsPerson)
        );
    }

    #[test]
    fn rejects_id_jag_for_wrong_resource() {
        let verifier = IdJagVerifier::new(
            IdJagConfig::new(
                TokenIssuer::new(ISSUER).unwrap(),
                TokenIssuer::new("https://veoveo.example/oauth").unwrap(),
                ProtectedResourceId::new(AUDIENCE).unwrap(),
                vec![Algorithm::RS256],
            )
            .unwrap(),
            jwks(),
        );

        let err = verifier
            .verify(&id_jag("https://veoveo.example/mcp/other", "id-jag-2"))
            .expect_err("wrong resource should fail");

        assert!(matches!(err, AuthError::InvalidIdentityAssertionResource));
    }

    #[test]
    fn verifies_oidc_id_token_and_maps_principal() {
        let verifier = OidcIdTokenVerifier::new(
            OidcIdTokenConfig::new(
                TokenIssuer::new(ISSUER).unwrap(),
                OidcClientId::new("veoveo").unwrap(),
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
        assert!(
            verified
                .principal
                .assurances
                .contains(&PrincipalAssurance::UsPerson)
        );
    }

    #[test]
    fn verifies_entra_oidc_aliases() {
        let claim_mapping = veoveo_mcp_contract::IdentityProviderClaimMapping {
            subject: veoveo_mcp_contract::IdentityProviderSubjectClaim::Oid,
            tenant: Some(veoveo_mcp_contract::IdentityProviderTenantClaimMapping {
                claim: veoveo_mcp_contract::IdentityProviderTenantClaim::Tid,
                values: std::collections::BTreeMap::from([(
                    "tenant-a".to_string(),
                    veoveo_mcp_contract::TenantId::new("tenant-a").unwrap(),
                )]),
            }),
        };
        let verifier = OidcIdTokenVerifier::new(
            OidcIdTokenConfig::new_with_claim_mapping(
                TokenIssuer::new(ISSUER).unwrap(),
                OidcClientId::new("veoveo").unwrap(),
                OidcNonce::new("nonce-1").unwrap(),
                vec![Algorithm::RS256],
                claim_mapping,
            )
            .unwrap(),
            jwks(),
        );

        let verified = verifier
            .verify(&oidc_entra_id_token("nonce-1"))
            .expect("valid Entra OIDC ID token");

        assert_eq!(verified.principal.subject.as_str(), "entra-object-id");
        assert_eq!(verified.principal.tenant.unwrap().as_str(), "tenant-a");
        assert!(
            verified
                .principal
                .roles
                .contains(&veoveo_mcp_contract::RoleId::new("veoveo_operator").unwrap())
        );
    }

    #[test]
    fn rejects_oidc_id_token_nonce_mismatch() {
        let verifier = OidcIdTokenVerifier::new(
            OidcIdTokenConfig::new(
                TokenIssuer::new(ISSUER).unwrap(),
                OidcClientId::new("veoveo").unwrap(),
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
