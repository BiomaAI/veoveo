//! Pure RSA OAuth client assertion signing. Transport and credential files belong to callers.
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use veoveo_types::{IdMetadata, IdProfile, IdProfileSpec, IdentifierError, OAuthClientId};
use zeroize::Zeroizing;

pub const CLIENT_ASSERTION_TYPE: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";
pub const ASSERTION_LIFETIME_SECONDS: u64 = 60;

/// Public key selector, admitted once for JWT headers and configuration.
#[veoveo_types::id(text(KeyIds))]
pub struct ClientAssertionKeyId(String);
#[doc(hidden)]
pub struct KeyIds;
impl IdProfile for KeyIds {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _: IdMetadata| {
        if value.is_empty()
            || value.len() > 128
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
        {
            return Err(IdentifierError::new(
                value,
                "key id requires 1..128 ASCII letters, digits, hyphens, underscores or dots",
            ));
        }
        Ok(())
    });
}
#[derive(Debug, thiserror::Error)]
pub enum AssertionError {
    #[error("invalid RSA client signing key")]
    InvalidKey,
    #[error("invalid client assertion time")]
    InvalidTime,
    #[error("client assertion requires an HTTPS token endpoint without credentials or fragment")]
    InvalidEndpoint,
    #[error("RSA client assertion signing failed")]
    Signing,
}

pub struct ClientAssertion(Zeroizing<String>);
impl ClientAssertion {
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for ClientAssertion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ClientAssertion([REDACTED])")
    }
}
pub struct ClientAssertionSigner {
    key_id: ClientAssertionKeyId,
    key: EncodingKey,
}
#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a OAuthClientId,
    sub: &'a OAuthClientId,
    aud: &'a str,
    iat: u64,
    nbf: u64,
    exp: u64,
    jti: &'a str,
}
impl ClientAssertionSigner {
    pub fn from_rsa_pem(key_id: ClientAssertionKeyId, pem: &[u8]) -> Result<Self, AssertionError> {
        let key = EncodingKey::from_rsa_pem(pem).map_err(|_| AssertionError::InvalidKey)?;
        Ok(Self::from_rsa_key(key_id, key))
    }
    /// The owner supplies its already admitted RSA key; fixtures may use DER.
    pub fn from_rsa_key(key_id: ClientAssertionKeyId, key: EncodingKey) -> Self {
        let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
        Self { key_id, key }
    }
    pub fn public_jwk(&self) -> Result<jsonwebtoken::jwk::Jwk, AssertionError> {
        let mut jwk = jsonwebtoken::jwk::Jwk::from_encoding_key(&self.key, Algorithm::RS256)
            .map_err(|_| AssertionError::InvalidKey)?;
        jwk.common.key_id = Some(self.key_id.to_string());
        Ok(jwk)
    }
    /// Worker assertions bind the exact canonical token endpoint and expire after 60 seconds.
    pub fn assertion(
        &self,
        client: &OAuthClientId,
        endpoint: &url::Url,
    ) -> Result<ClientAssertion, AssertionError> {
        if endpoint.scheme() != "https"
            || endpoint.host_str().is_none()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(AssertionError::InvalidEndpoint);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| AssertionError::InvalidTime)?
            .as_secs();
        self.diagnostic_assertion(
            client,
            endpoint.as_str(),
            now,
            now.checked_add(ASSERTION_LIFETIME_SECONDS)
                .ok_or(AssertionError::InvalidTime)?,
            &uuid::Uuid::new_v4().to_string(),
        )
    }
    /// Explicit diagnostic claims preserve owner negative probes; callers own their transport admission.
    pub fn diagnostic_assertion(
        &self,
        client: &OAuthClientId,
        audience: &str,
        issued_at: u64,
        expires_at: u64,
        jwt_id: &str,
    ) -> Result<ClientAssertion, AssertionError> {
        let claims = Claims {
            iss: client,
            sub: client,
            aud: audience,
            iat: issued_at,
            nbf: issued_at,
            exp: expires_at,
            jti: jwt_id,
        };
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.key_id.to_string());
        encode(&header, &claims, &self.key)
            .map(|token| ClientAssertion(Zeroizing::new(token)))
            .map_err(|_| AssertionError::Signing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs8::EncodePrivateKey;

    #[test]
    fn worker_assertions_bind_client_endpoint_key_and_fresh_short_identity() {
        let key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let pem = key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).unwrap();
        let signer = ClientAssertionSigner::from_rsa_pem(
            ClientAssertionKeyId::parse("worker-key").unwrap(),
            pem.as_bytes(),
        )
        .unwrap();
        let client = OAuthClientId::parse("worker").unwrap();
        let endpoint = url::Url::parse("https://issuer.fixture/oauth/token").unwrap();
        let first = signer.assertion(&client, &endpoint).unwrap();
        let second = signer.assertion(&client, &endpoint).unwrap();
        assert_eq!(format!("{first:?}"), "ClientAssertion([REDACTED])");
        let header = jsonwebtoken::decode_header(first.expose_secret()).unwrap();
        assert_eq!(header.alg, Algorithm::RS256);
        assert_eq!(header.kid.as_deref(), Some("worker-key"));
        let jwk = signer.public_jwk().unwrap();
        let public = serde_json::to_value(&jwk).unwrap();
        assert!(public.get("d").is_none());
        let key = jsonwebtoken::DecodingKey::from_jwk(&jwk).unwrap();
        let mut validation = jsonwebtoken::Validation::new(Algorithm::RS256);
        validation.set_issuer(&[client.as_str()]);
        validation.set_audience(&[endpoint.as_str()]);
        let first =
            jsonwebtoken::decode::<serde_json::Value>(first.expose_secret(), &key, &validation)
                .unwrap()
                .claims;
        let second =
            jsonwebtoken::decode::<serde_json::Value>(second.expose_secret(), &key, &validation)
                .unwrap()
                .claims;
        assert_eq!(first["sub"], client.as_str());
        assert_eq!(first["nbf"], first["iat"]);
        assert_eq!(
            first["exp"].as_u64().unwrap() - first["iat"].as_u64().unwrap(),
            60
        );
        assert_ne!(first["jti"], second["jti"]);
        uuid::Uuid::parse_str(first["jti"].as_str().unwrap()).unwrap();
        for endpoint in [
            "http://issuer.fixture/token",
            "https://user@issuer.fixture/token",
            "https://issuer.fixture/token#fragment",
        ] {
            assert!(matches!(
                signer.assertion(&client, &url::Url::parse(endpoint).unwrap()),
                Err(AssertionError::InvalidEndpoint)
            ));
        }
        let diagnostic = signer
            .diagnostic_assertion(
                &client,
                "chosen-negative-audience",
                10,
                20,
                "chosen-replay-id",
            )
            .unwrap();
        validation.validate_exp = false;
        validation.validate_aud = false;
        let claims = jsonwebtoken::decode::<serde_json::Value>(
            diagnostic.expose_secret(),
            &key,
            &validation,
        )
        .unwrap()
        .claims;
        assert_eq!(claims["aud"], "chosen-negative-audience");
        assert_eq!(claims["jti"], "chosen-replay-id");
        assert_eq!(claims["exp"], 20);
    }

    #[test]
    fn key_admission_and_errors_preserve_redaction() {
        for key_id in ["", "bad key", "bad/key"] {
            assert!(ClientAssertionKeyId::parse(key_id).is_err());
        }
        let error = ClientAssertionSigner::from_rsa_pem(
            ClientAssertionKeyId::parse("key").unwrap(),
            b"private malformed key",
        )
        .err()
        .unwrap();
        assert_eq!(error.to_string(), "invalid RSA client signing key");
    }
}
