//! Machine JWT signing uses Veoveo's required key ID; MCP stays on the maintained SDK.
use super::{IndexingConfig, SigningAlgorithm, control::Selection};
use crate::ServiceError;
use jsonwebtoken::{EncodingKey, Header};
use rmcp::{
    ClientServiceExt, RoleClient,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::Instant;

pub(super) struct Connection {
    pub service: RunningService<RoleClient, ()>,
    pub rotate_at: Instant,
}
#[derive(Serialize)]
struct Assertion<'a> {
    iss: &'a str,
    sub: &'a str,
    aud: &'a str,
    iat: i64,
    nbf: i64,
    exp: i64,
    jti: uuid::Uuid,
}
#[derive(Deserialize)]
struct Token {
    access_token: String,
    token_type: String,
    expires_in: u64,
    scope: Option<String>,
}

fn endpoint(value: &str) -> Result<url::Url, ServiceError> {
    let url = url::Url::parse(value).map_err(|_| ServiceError::MachineConfiguration)?;
    let loopback = url
        .host_str()
        .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1"));
    if !(url.scheme() == "https" || (url.scheme() == "http" && loopback))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.query().is_some()
    {
        return Err(ServiceError::MachineConfiguration);
    }
    Ok(url)
}

pub(super) async fn connect(
    config: &IndexingConfig,
    selected: &Selection,
) -> Result<Connection, ServiceError> {
    tokio::time::timeout(Duration::from_secs(30), connect_inner(config, selected))
        .await
        .map_err(|_| ServiceError::Deadline)?
}
async fn connect_inner(
    config: &IndexingConfig,
    selected: &Selection,
) -> Result<Connection, ServiceError> {
    let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
    let token_endpoint = endpoint(selected.token_endpoint.as_str())?;
    let resource = endpoint(selected.resource.as_str())?;
    let mut http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10));
    if let Some(path) = &config.trusted_ca_file {
        let pem = tokio::fs::read(path)
            .await
            .map_err(|_| ServiceError::MachineConfiguration)?;
        for cert in reqwest::Certificate::from_pem_bundle(&pem)
            .map_err(|_| ServiceError::MachineConfiguration)?
        {
            http = http.add_root_certificate(cert);
        }
    }
    let http = http
        .build()
        .map_err(|_| ServiceError::MachineConfiguration)?;
    let pem = secrecy::SecretBox::new(
        tokio::fs::read(&config.private_key_file)
            .await
            .map_err(|_| ServiceError::MachineConfiguration)?
            .into_boxed_slice(),
    );
    use secrecy::ExposeSecret;
    let (algorithm, key) = match config.signing_algorithm {
        SigningAlgorithm::Rs256 => (
            jsonwebtoken::Algorithm::RS256,
            EncodingKey::from_rsa_pem(pem.expose_secret()),
        ),
        SigningAlgorithm::Es256 => (
            jsonwebtoken::Algorithm::ES256,
            EncodingKey::from_ec_pem(pem.expose_secret()),
        ),
        SigningAlgorithm::EdDsa => (
            jsonwebtoken::Algorithm::EdDSA,
            EncodingKey::from_ed_pem(pem.expose_secret()),
        ),
    };
    let key = key.map_err(|_| ServiceError::MachineConfiguration)?;
    let mut header = Header::new(algorithm);
    header.kid = Some(config.key_id.to_string());
    let now = chrono::Utc::now().timestamp();
    let assertion = jsonwebtoken::encode(
        &header,
        &Assertion {
            iss: config.client_id.as_str(),
            sub: config.client_id.as_str(),
            aud: token_endpoint.as_str(),
            iat: now,
            nbf: now,
            exp: now + 120,
            jti: uuid::Uuid::now_v7(),
        },
        &key,
    )
    .map_err(|_| ServiceError::MachineAuthentication)?;
    let requested_scopes = selected
        .scopes
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let started = Instant::now();
    let mut response = http
        .post(token_endpoint)
        .form(&[
            ("grant_type", "client_credentials"),
            ("client_id", config.client_id.as_str()),
            ("work_context", selected.work_context.as_str()),
            ("resource", selected.resource.as_str()),
            ("scope", requested_scopes.as_str()),
            (
                "client_assertion_type",
                "urn:ietf:params:oauth:client-assertion-type:jwt-bearer",
            ),
            ("client_assertion", assertion.as_str()),
        ])
        .send()
        .await
        .map_err(|_| ServiceError::MachineAuthentication)?;
    if !response.status().is_success() {
        return Err(ServiceError::MachineAuthentication);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ServiceError::MachineAuthentication)?
    {
        if bytes.len() + chunk.len() > 64 * 1024 {
            return Err(ServiceError::MachineAuthentication);
        }
        bytes.extend_from_slice(&chunk);
    }
    let token: Token =
        serde_json::from_slice(&bytes).map_err(|_| ServiceError::MachineAuthentication)?;
    if token.access_token.is_empty()
        || token.access_token.chars().any(char::is_whitespace)
        || !token.token_type.eq_ignore_ascii_case("bearer")
        || !(2..=86_400).contains(&token.expires_in)
        || token.scope.as_ref().is_some_and(|scopes| {
            scopes
                .split_whitespace()
                .collect::<std::collections::BTreeSet<_>>()
                != selected.scopes.iter().map(|scope| scope.as_str()).collect()
        })
    {
        return Err(ServiceError::MachineAuthentication);
    }
    let rotate_at = started + Duration::from_millis(token.expires_in * 800);
    let transport = StreamableHttpClientTransport::with_client(
        http,
        StreamableHttpClientTransportConfig::with_uri(resource.as_str())
            .auth_header(token.access_token),
    );
    let service = ()
        .serve_with_lifecycle(
            transport,
            rmcp::ClientLifecycleMode::Discover {
                preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .map_err(|_| ServiceError::SourceUnavailable)?;
    if Instant::now() >= rotate_at {
        return Err(ServiceError::Deadline);
    }
    Ok(Connection { service, rotate_at })
}
