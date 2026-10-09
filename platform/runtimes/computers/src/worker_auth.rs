//! Dedicated external worker credentials. Provider mutations are never retried.
use crate::{
    Result, RuntimeFailure,
    client::{read_file, validate_path},
};
use oauth2::{
    AuthType, ClientId, EndpointNotSet, EndpointSet, Scope, TokenResponse, TokenUrl,
    basic::{BasicClient, BasicTokenType},
};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{sync::Mutex, time::Instant};
use tonic::{body::Body, codegen::http, transport::Channel};
use tower::{Service, ServiceExt};
use veoveo_gateway_contract::{OAuthClientAuthMethod, ProtectedResourceId};
use veoveo_oauth_client::{CLIENT_ASSERTION_TYPE, ClientAssertionKeyId, ClientAssertionSigner};
use veoveo_types::{Check, Checked, HttpsUrl, OAuthClientId, ScopeName};
use zeroize::Zeroizing;

const AUTH_BUDGET: Duration = Duration::from_secs(10);
const MAX_RESPONSE: usize = 64 * 1024;
const REFRESH_MARGIN: Duration = Duration::from_secs(30);

fn worker_http_builder() -> reqwest::ClientBuilder {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(AUTH_BUDGET)
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkerOAuthFields {
    pub issuer: HttpsUrl,
    pub resource: ProtectedResourceId,
    pub client_id: OAuthClientId,
    pub private_key_file: PathBuf,
    pub key_id: ClientAssertionKeyId,
    pub token_endpoint_auth_method: OAuthClientAuthMethod,
    pub scopes: BTreeSet<ScopeName>,
    pub ca_file: Option<PathBuf>,
}
impl Check for WorkerOAuthFields {
    type Error = RuntimeFailure;
    fn check(&self) -> Result<()> {
        if self.issuer.as_url().query().is_some()
            || self.resource.as_str().is_empty()
            || self.resource.as_str().len() > 256
            || self
                .resource
                .as_str()
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
            || self.client_id.as_str().len() > 256
            || self.scopes.is_empty()
            || self.scopes.len() > 16
        {
            return Err(RuntimeFailure::InvalidWorkerAuthentication);
        }
        if self.token_endpoint_auth_method != OAuthClientAuthMethod::PrivateKeyJwt
            || HttpsUrl::parse(self.resource.as_str())
                .map(|url| url.as_str() != self.resource.as_str())
                .unwrap_or(true)
        {
            return Err(RuntimeFailure::InvalidWorkerAuthentication);
        }
        validate_path(&self.private_key_file)?;
        if let Some(path) = &self.ca_file {
            validate_path(path)?;
        }
        Ok(())
    }
}
/// Immutable admission applies to constructors and decoding alike.
#[derive(Clone, Deserialize)]
#[serde(transparent)]
pub struct WorkerOAuthConfig(Checked<WorkerOAuthFields>);
impl WorkerOAuthConfig {
    pub fn new(fields: WorkerOAuthFields) -> Result<Self> {
        Checked::new(fields).map(Self)
    }
    pub fn issuer(&self) -> &HttpsUrl {
        &self.0.issuer
    }
    pub fn resource(&self) -> &ProtectedResourceId {
        &self.0.resource
    }
    pub(crate) async fn validate_files(&self) -> Result<()> {
        let bytes = read_private_key(&self.0.private_key_file).await?;
        ClientAssertionSigner::from_rsa_pem(self.0.key_id.clone(), &bytes)
            .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?;
        if let Some(path) = &self.0.ca_file {
            let pem = read_file(path).await?;
            if reqwest::Certificate::from_pem_bundle(&pem)
                .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?
                .is_empty()
            {
                return Err(RuntimeFailure::InvalidWorkerAuthentication);
            }
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct Discovery {
    issuer: HttpsUrl,
    token_endpoint: HttpsUrl,
    token_endpoint_auth_methods_supported: Vec<String>,
}
type OAuthClient =
    BasicClient<EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;
struct CachedToken {
    value: Zeroizing<String>,
    refresh_at: Instant,
}
#[derive(Clone)]
pub(crate) struct WorkerTokens {
    client: OAuthClient,
    http: reqwest::Client,
    scopes: BTreeSet<ScopeName>,
    cache: Arc<Mutex<Option<CachedToken>>>,
    issuer: HttpsUrl,
    resource: ProtectedResourceId,
    client_id: OAuthClientId,
    signer: Arc<ClientAssertionSigner>,
    token_endpoint: HttpsUrl,
}
impl WorkerTokens {
    pub(crate) async fn connect(config: WorkerOAuthConfig) -> Result<Self> {
        tokio::time::timeout(AUTH_BUDGET, Self::initialize(config))
            .await
            .map_err(|_| RuntimeFailure::WorkerAuthenticationUnavailable)?
    }
    async fn initialize(config: WorkerOAuthConfig) -> Result<Self> {
        config.0.check()?;
        let mut builder = worker_http_builder();
        if let Some(path) = &config.0.ca_file {
            let pem = read_file(path).await?;
            let certificates = reqwest::Certificate::from_pem_bundle(&pem)
                .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?;
            if certificates.is_empty() {
                return Err(RuntimeFailure::InvalidWorkerAuthentication);
            }
            for cert in certificates {
                builder = builder.add_root_certificate(cert);
            }
        }
        let http = builder
            .build()
            .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?;
        let mut discovery_url = config.0.issuer.as_url().clone();
        discovery_url
            .path_segments_mut()
            .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?
            .pop_if_empty()
            .push(".well-known")
            .push("openid-configuration");
        let response = http
            .get(discovery_url)
            .send()
            .await
            .map_err(|_| RuntimeFailure::WorkerAuthenticationUnavailable)?;
        if !response.status().is_success() {
            return Err(RuntimeFailure::WorkerAuthenticationUnavailable);
        }
        let discovery: Discovery = serde_json::from_slice(&bounded_response(response).await?)
            .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?;
        if discovery.issuer != config.0.issuer
            || discovery.token_endpoint.as_url().origin() != config.0.issuer.as_url().origin()
            || !discovery
                .token_endpoint_auth_methods_supported
                .iter()
                .any(|method| {
                    OAuthClientAuthMethod::deserialize(serde::de::value::StrDeserializer::<
                        serde::de::value::Error,
                    >::new(method))
                    .is_ok_and(|method| method == OAuthClientAuthMethod::PrivateKeyJwt)
                })
        {
            return Err(RuntimeFailure::InvalidWorkerAuthentication);
        }
        let pem = read_private_key(&config.0.private_key_file).await?;
        let signer = Arc::new(
            ClientAssertionSigner::from_rsa_pem(config.0.key_id.clone(), &pem)
                .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?,
        );
        let client = BasicClient::new(ClientId::new(config.0.client_id.to_string()))
            .set_auth_type(AuthType::RequestBody)
            .set_token_uri(
                TokenUrl::new(discovery.token_endpoint.to_string())
                    .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?,
            );
        Ok(Self {
            client,
            http,
            scopes: config.0.scopes.clone(),
            cache: Arc::new(Mutex::new(None)),
            issuer: config.0.issuer.clone(),
            resource: config.0.resource.clone(),
            client_id: config.0.client_id.clone(),
            signer,
            token_endpoint: discovery.token_endpoint,
        })
    }
    pub(crate) async fn token(&self) -> Result<Zeroizing<String>> {
        tokio::time::timeout(AUTH_BUDGET, self.acquire())
            .await
            .map_err(|_| RuntimeFailure::WorkerAuthenticationUnavailable)?
    }
    async fn acquire(&self) -> Result<Zeroizing<String>> {
        let mut cache = self.cache.lock().await;
        if let Some(token) = &*cache
            && token.refresh_at > Instant::now()
            && issued_lifetime(&token.value, &self.issuer, self.resource.as_str()).is_ok()
        {
            return Ok(token.value.clone());
        }
        let issued = Instant::now();
        let assertion = self
            .signer
            .assertion(&self.client_id, self.token_endpoint.as_url())
            .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?;
        let request = self
            .client
            .exchange_client_credentials()
            .add_extra_param("resource", self.resource.to_string())
            .add_extra_param("client_assertion_type", CLIENT_ASSERTION_TYPE)
            .add_extra_param("client_assertion", assertion.expose_secret())
            .add_scopes(
                self.scopes
                    .iter()
                    .map(|scope| Scope::new(scope.to_string())),
            );
        let adapter = OAuthHttp(self.http.clone());
        let response = request
            .request_async(&adapter)
            .await
            .map_err(|_| RuntimeFailure::WorkerAuthenticationUnavailable)?;
        let lifetime = response
            .expires_in()
            .filter(|expires| *expires > REFRESH_MARGIN && *expires <= Duration::from_secs(86400))
            .ok_or(RuntimeFailure::InvalidWorkerAuthentication)?;
        if response.token_type() != &BasicTokenType::Bearer {
            return Err(RuntimeFailure::InvalidWorkerAuthentication);
        }
        let value = Zeroizing::new(response.access_token().secret().clone());
        if !crate::remote_access::valid_token(&value) {
            return Err(RuntimeFailure::InvalidWorkerAuthentication);
        }
        let jwt_lifetime = issued_lifetime(&value, &self.issuer, self.resource.as_str())?;
        let refresh_at = issued
            .checked_add(lifetime)
            .zip(Instant::now().checked_add(jwt_lifetime))
            .map(|(response_expiry, jwt_expiry)| response_expiry.min(jwt_expiry))
            .and_then(|expiry| expiry.checked_sub(REFRESH_MARGIN))
            .filter(|time| *time > Instant::now())
            .ok_or(RuntimeFailure::InvalidWorkerAuthentication)?;
        *cache = Some(CachedToken {
            value: value.clone(),
            refresh_at,
        });
        Ok(value)
    }
}
#[derive(Deserialize)]
struct IssuedClaims {
    iss: HttpsUrl,
    aud: IssuedAudience,
    exp: u64,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum IssuedAudience {
    One(String),
    Many(Vec<String>),
}
fn issued_lifetime(token: &str, issuer: &HttpsUrl, audience: &str) -> Result<Duration> {
    // Configuration admission of a JWT received from the authenticated token
    // endpoint. This intentionally supplies no signature or role authority;
    // stock OpenShell verifies and authorizes the same token at every RPC.
    let claims = jsonwebtoken::dangerous::insecure_decode::<IssuedClaims>(token)
        .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?
        .claims;
    let matching_audience = match claims.aud {
        IssuedAudience::One(value) => value == audience,
        IssuedAudience::Many(values) => values.as_slice() == [audience],
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?
        .as_secs();
    let lifetime = claims
        .exp
        .checked_sub(now)
        .filter(|seconds| *seconds > 30 && *seconds <= 86400)
        .ok_or(RuntimeFailure::InvalidWorkerAuthentication)?;
    if claims.iss != *issuer || !matching_audience {
        return Err(RuntimeFailure::InvalidWorkerAuthentication);
    }
    Ok(Duration::from_secs(lifetime))
}

async fn read_private_key(path: &std::path::Path) -> Result<Zeroizing<Vec<u8>>> {
    use tokio::io::AsyncReadExt;
    let mut options = tokio::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits());
    // Follow Kubernetes ..data projections, then admit the opened inode once.
    let file = options
        .open(path)
        .await
        .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?;
    let metadata = file
        .metadata()
        .await
        .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if !matches!(metadata.permissions().mode() & 0o777, 0o400 | 0o600 | 0o440) {
            return Err(RuntimeFailure::InvalidWorkerAuthentication);
        }
    }
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 16 * 1024 {
        return Err(RuntimeFailure::InvalidWorkerAuthentication);
    }
    let mut bytes = Zeroizing::new(Vec::new());
    file.take(16 * 1024 + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?;
    if bytes.is_empty() || bytes.len() > 16 * 1024 {
        return Err(RuntimeFailure::InvalidWorkerAuthentication);
    }
    Ok(bytes)
}
struct OAuthHttp(reqwest::Client);
impl<'c> oauth2::AsyncHttpClient<'c> for OAuthHttp {
    type Error = OAuthTransportFailure;
    type Future = Pin<
        Box<
            dyn Future<Output = std::result::Result<oauth2::HttpResponse, Self::Error>> + Send + 'c,
        >,
    >;
    fn call(&'c self, request: oauth2::HttpRequest) -> Self::Future {
        let client = self.0.clone();
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let response = client
                .request(parts.method, parts.uri.to_string())
                .headers(parts.headers)
                .body(body)
                .send()
                .await
                .map_err(|_| OAuthTransportFailure)?;
            let status = response.status();
            let headers = response.headers().clone();
            let bytes = bounded_response(response)
                .await
                .map_err(|_| OAuthTransportFailure)?;
            let mut response = http::Response::new(bytes);
            *response.status_mut() = status;
            *response.headers_mut() = headers;
            Ok(response)
        })
    }
}

async fn bounded_response(mut response: reqwest::Response) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| RuntimeFailure::WorkerAuthenticationUnavailable)?
    {
        if chunk.len() > MAX_RESPONSE - bytes.len() {
            return Err(RuntimeFailure::InvalidWorkerAuthentication);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
#[derive(Debug, thiserror::Error)]
#[error("worker OAuth transport failed; provider dispatch was not retried")]
struct OAuthTransportFailure;

#[derive(Clone)]
pub(crate) struct WorkerChannel {
    channel: Channel,
    tokens: WorkerTokens,
}
impl WorkerChannel {
    pub(crate) fn new(channel: Channel, tokens: WorkerTokens) -> Self {
        Self { channel, tokens }
    }
}
impl Service<http::Request<Body>> for WorkerChannel {
    type Response = http::Response<Body>;
    type Error = RuntimeFailure;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response>> + Send>>;
    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn call(&mut self, mut request: http::Request<Body>) -> Self::Future {
        let tokens = self.tokens.clone();
        let channel = self.channel.clone();
        Box::pin(async move {
            let token = tokens.token().await?;
            let mut header = http::HeaderValue::from_str(&format!("Bearer {}", *token))
                .map_err(|_| RuntimeFailure::InvalidWorkerAuthentication)?;
            header.set_sensitive(true);
            request
                .headers_mut()
                .insert(http::header::AUTHORIZATION, header);
            channel
                .oneshot(request)
                .await
                .map_err(|_| RuntimeFailure::Unavailable)
        })
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    #[test]
    fn worker_http_client_initializes_tls_without_ambient_service_startup() {
        worker_http_builder()
            .build()
            .expect("worker HTTP client constructs in a fresh process");
    }
    fn token(issuer: &str, audience: serde_json::Value, expiry: u64) -> String {
        let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256),
            &serde_json::json!({"iss":issuer,"aud":audience,"exp":expiry}),
            &jsonwebtoken::EncodingKey::from_secret(b"fixture-only-signature"),
        )
        .unwrap()
    }
    #[test]
    fn issued_profile_checks_configuration_and_expiry_without_authorizing_signature_or_roles() {
        let issuer = HttpsUrl::parse("https://issuer.fixture/").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let valid = token(issuer.as_str(), serde_json::json!("resource"), now + 3600);
        assert!(issued_lifetime(&valid, &issuer, "resource").is_ok());
        // The issuer endpoint is authenticated by TLS. This local decode does
        // not validate signatures or roles; OpenShell must reject invalid ones.
        let (head_and_claims, _) = valid.rsplit_once('.').unwrap();
        let altered_signature = format!("{head_and_claims}.YWx0ZXJlZA");
        assert!(issued_lifetime(&altered_signature, &issuer, "resource").is_ok());
        for (iss, aud, exp) in [
            (
                "https://foreign.fixture/",
                serde_json::json!("resource"),
                now + 3600,
            ),
            (issuer.as_str(), serde_json::json!("foreign"), now + 3600),
            (
                issuer.as_str(),
                serde_json::json!(["resource", "foreign"]),
                now + 3600,
            ),
            (issuer.as_str(), serde_json::json!("resource"), now),
            (issuer.as_str(), serde_json::json!("resource"), now + 30),
        ] {
            assert!(issued_lifetime(&token(iss, aud, exp), &issuer, "resource").is_err());
        }
        assert!(issued_lifetime("private malformed token", &issuer, "resource").is_err());
        assert!(
            !RuntimeFailure::InvalidWorkerAuthentication
                .to_string()
                .contains("private malformed token")
        );
    }
    #[tokio::test]
    async fn secret_projection_follows_symlinks_and_admits_group_read_on_one_opened_inode() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory =
            std::env::temp_dir().join(format!("veoveo-worker-secret-{}", uuid::Uuid::now_v7()));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(directory.clone());
        std::fs::create_dir(&directory).unwrap();
        std::fs::create_dir(directory.join("..generation")).unwrap();
        let secret = directory.join("..generation/secret");
        std::fs::write(&secret, "fixture-secret").unwrap();
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o440)).unwrap();
        symlink("..generation", directory.join("..data")).unwrap();
        symlink("..data/secret", directory.join("secret")).unwrap();
        let projected = directory.join("secret");
        assert_eq!(
            &*read_private_key(&projected).await.unwrap(),
            b"fixture-secret"
        );
        for mode in [0o444, 0o460] {
            std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(mode)).unwrap();
            assert!(read_private_key(&projected).await.is_err());
        }
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&secret, vec![b'x'; 16385]).unwrap();
        assert!(read_private_key(&projected).await.is_err());
    }
}
