use std::{path::Path, sync::Arc};

use anyhow::{Context, Result, ensure};
use chrono::Utc;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use url::Url;

use crate::config::ClientAssertionAlgorithm;

const CLIENT_ASSERTION_TYPE: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

#[derive(Debug, Clone, Deserialize)]
pub struct AuthorizationServerMetadata {
    pub issuer: String,
    pub token_endpoint: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
}

#[derive(Debug, Serialize)]
struct ClientAssertionClaims<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
    jti: String,
}

#[derive(Clone)]
pub struct OAuthTokenProvider {
    http: reqwest::Client,
    token_endpoint: Url,
    token_transport_endpoint: Url,
    protected_resource: Url,
    client_id: String,
    key_id: String,
    algorithm: Algorithm,
    encoding_key: Arc<EncodingKey>,
    cached: Arc<Mutex<Option<CachedToken>>>,
}

struct CachedToken {
    value: SecretString,
    expires_at: chrono::DateTime<Utc>,
}

impl OAuthTokenProvider {
    pub fn new(
        http: reqwest::Client,
        token_endpoint: Url,
        token_transport_endpoint: Url,
        protected_resource: Url,
        client_id: String,
        key_id: String,
        algorithm: ClientAssertionAlgorithm,
        private_key_pem_file: &Path,
    ) -> Result<Self> {
        ensure!(
            token_endpoint.scheme() == "https"
                || (token_endpoint.scheme() == "http"
                    && token_endpoint
                        .host_str()
                        .is_some_and(|host| { matches!(host, "localhost" | "127.0.0.1" | "::1") })),
            "OAuth token endpoint must use HTTPS or loopback HTTP"
        );
        ensure!(
            matches!(token_transport_endpoint.scheme(), "http" | "https")
                && token_transport_endpoint.host_str().is_some(),
            "OAuth token transport endpoint must use HTTP(S)"
        );
        let pem = std::fs::read(private_key_pem_file).with_context(|| {
            format!(
                "reading producer private key {}",
                private_key_pem_file.display()
            )
        })?;
        let (algorithm, encoding_key) = match algorithm {
            ClientAssertionAlgorithm::Rs256 => (Algorithm::RS256, EncodingKey::from_rsa_pem(&pem)?),
            ClientAssertionAlgorithm::Es256 => (Algorithm::ES256, EncodingKey::from_ec_pem(&pem)?),
            ClientAssertionAlgorithm::EdDsa => (Algorithm::EdDSA, EncodingKey::from_ed_pem(&pem)?),
        };
        Ok(Self {
            http,
            token_endpoint,
            token_transport_endpoint,
            protected_resource,
            client_id,
            key_id,
            algorithm,
            encoding_key: Arc::new(encoding_key),
            cached: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn access_token(&self) -> Result<SecretString> {
        let mut cached = self.cached.lock().await;
        if let Some(token) = cached.as_ref()
            && token.expires_at > Utc::now() + chrono::TimeDelta::seconds(30)
        {
            return Ok(token.value.clone());
        }
        let now = Utc::now();
        let assertion = encode(
            &Header {
                alg: self.algorithm,
                kid: Some(self.key_id.clone()),
                ..Default::default()
            },
            &ClientAssertionClaims {
                iss: &self.client_id,
                sub: &self.client_id,
                aud: self.token_endpoint.as_str(),
                iat: now.timestamp(),
                exp: (now + chrono::TimeDelta::minutes(2)).timestamp(),
                jti: uuid::Uuid::now_v7().to_string(),
            },
            &self.encoding_key,
        )?;
        let response = self
            .http
            .post(self.token_transport_endpoint.clone())
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.client_id.as_str()),
                ("client_assertion_type", CLIENT_ASSERTION_TYPE),
                ("client_assertion", assertion.as_str()),
                ("scope", "recording:ingest"),
                ("resource", self.protected_resource.as_str()),
            ])
            .send()
            .await
            .context("requesting recording producer access token")?
            .error_for_status()
            .context("recording producer token request was denied")?
            .json::<TokenResponse>()
            .await
            .context("decoding recording producer token response")?;
        ensure!(
            response.token_type.eq_ignore_ascii_case("Bearer") && response.expires_in > 0,
            "authorization server returned an invalid token response"
        );
        let expires_at =
            now + chrono::TimeDelta::seconds(i64::try_from(response.expires_in.min(86_400))?);
        let value = SecretString::from(response.access_token);
        *cached = Some(CachedToken {
            value: value.clone(),
            expires_at,
        });
        Ok(value)
    }

    pub async fn invalidate(&self, token: &SecretString) {
        let mut cached = self.cached.lock().await;
        if cached
            .as_ref()
            .is_some_and(|cached| cached.value.expose_secret() == token.expose_secret())
        {
            *cached = None;
        }
    }
}

pub fn authorization_server_metadata_url(issuer: &Url) -> Result<Url> {
    ensure!(
        issuer.query().is_none() && issuer.fragment().is_none(),
        "authorization-server issuer must not contain a query or fragment"
    );
    let mut metadata = issuer.clone();
    let issuer_path = issuer.path().trim_matches('/');
    let path = if issuer_path.is_empty() {
        "/.well-known/oauth-authorization-server".to_owned()
    } else {
        // RFC 8414 inserts the well-known component before an issuer path.
        format!("/.well-known/oauth-authorization-server/{issuer_path}")
    };
    metadata.set_path(&path);
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_rfc8414_metadata_url_for_gateway_issuer_path() {
        assert_eq!(
            authorization_server_metadata_url(&Url::parse("https://veoveo.example/oauth").unwrap())
                .unwrap()
                .as_str(),
            "https://veoveo.example/.well-known/oauth-authorization-server/oauth"
        );
    }
}
