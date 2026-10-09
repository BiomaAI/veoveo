//! HTTPS OAuth fixtures establish worker transport, not native provider policy.
use super::{WorkerOAuthConfig, WorkerOAuthFields};
use axum::{
    Form, Json, Router,
    extract::State,
    routing::{get, post},
};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

/// Select the workspace's pinned TLS implementation before native HTTP or HTTPS.
pub fn initialize_tls() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Clone, Copy)]
pub enum WorkerTokenProbe {
    Authorized,
    WrongSignature,
    WrongIssuer,
    WrongAudience,
    Expired,
    UnauthorizedRoles,
}

#[derive(Clone)]
struct IssuerState {
    issuer: veoveo_types::HttpsUrl,
    requests: Arc<AtomicUsize>,
    token: Arc<String>,
    lifetime: Arc<AtomicU64>,
    client_key: jsonwebtoken::DecodingKey,
    client_key_id: veoveo_oauth_client::ClientAssertionKeyId,
    replay: Arc<std::sync::Mutex<std::collections::BTreeMap<String, u64>>>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    grant_type: String,
    client_id: veoveo_types::OAuthClientId,
    client_assertion_type: String,
    client_assertion: String,
    resource: veoveo_gateway_contract::ProtectedResourceId,
    scope: String,
}
pub struct TestIssuer {
    pub config: WorkerOAuthConfig,
    #[allow(dead_code)] // Native targets use provider verification; unit targets inspect refresh.
    pub requests: Arc<AtomicUsize>,
    #[allow(dead_code)] // Unit gRPC fixture checks the exact issued bearer.
    pub token: Arc<String>,
    #[allow(dead_code)] // Unit targets control token endpoint expiry.
    pub lifetime: Arc<AtomicU64>,
    signing: jsonwebtoken::EncodingKey,
    private_key_file: PathBuf,
    handle: axum_server::Handle<std::net::SocketAddr>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl TestIssuer {
    #[allow(dead_code)] // Host supplies its inspected bridge through start_on.
    pub async fn start(dir: &Path, server_cert: &str, server_key: &str) -> Self {
        Self::listen(
            dir,
            std::net::Ipv4Addr::LOCALHOST,
            server_cert,
            server_key,
            true,
        )
        .await
    }
    /// Bind only the inspected private interface; trust and SAN belong to this fixture.
    #[allow(dead_code)] // Unit transport controls supply their own TLS fixture.
    pub async fn start_on(dir: &Path, address: std::net::Ipv4Addr) -> Self {
        use rcgen::{
            BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
            KeyUsagePurpose,
        };
        use std::os::unix::fs::PermissionsExt;
        assert!(address.is_private() && !address.is_loopback());
        std::fs::create_dir(dir).unwrap();
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut ca = CertificateParams::new(vec![]).unwrap();
        ca.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        ca.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        let root_key = KeyPair::generate().unwrap();
        std::fs::write(dir.join("ca.pem"), ca.self_signed(&root_key).unwrap().pem()).unwrap();
        std::fs::set_permissions(dir.join("ca.pem"), std::fs::Permissions::from_mode(0o600))
            .unwrap();
        let authority = Issuer::new(ca, root_key);
        let mut leaf = CertificateParams::new(vec![address.to_string()]).unwrap();
        leaf.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        let key = KeyPair::generate().unwrap();
        let cert = leaf.signed_by(&key, &authority).unwrap();
        Self::listen(dir, address, &cert.pem(), &key.serialize_pem(), false).await
    }
    async fn listen(
        dir: &Path,
        address: std::net::Ipv4Addr,
        server_cert: &str,
        server_key: &str,
        localhost_name: bool,
    ) -> Self {
        initialize_tls();
        let private_key_file = dir.join("worker-private-key.pem");
        use rsa::pkcs8::EncodePrivateKey;
        let client_key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let pem = client_key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).unwrap();
        use std::os::unix::fs::OpenOptionsExt;
        let mut private_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&private_key_file)
            .unwrap();
        std::io::Write::write_all(&mut private_file, pem.as_bytes()).unwrap();
        let client_key_id = veoveo_oauth_client::ClientAssertionKeyId::parse(format!(
            "fixture-client-{}",
            uuid::Uuid::new_v4()
        ))
        .unwrap();
        let client_signer = veoveo_oauth_client::ClientAssertionSigner::from_rsa_pem(
            client_key_id.clone(),
            pem.as_bytes(),
        )
        .unwrap();
        let client_jwk = client_signer.public_jwk().unwrap();
        let client_key = jsonwebtoken::DecodingKey::from_jwk(&client_jwk).unwrap();
        let listener = std::net::TcpListener::bind((address, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut url = reqwest::Url::parse("https://localhost/").unwrap();
        if !localhost_name {
            url.set_ip_host(address.into()).unwrap();
        }
        url.set_port(Some(listener.local_addr().unwrap().port()))
            .unwrap();
        let issuer = veoveo_types::HttpsUrl::parse(url.as_str()).unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let lifetime = Arc::new(AtomicU64::new(3600));
        let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
        let expiration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600;
        let signing = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
        use base64::Engine;
        let jwks = serde_json::json!({"keys":[{"kty":"OKP","crv":"Ed25519","alg":"EdDSA","use":"sig","kid":"fixture-worker","x":base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signing.public_key_raw())}]});
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
        header.kid = Some("fixture-worker".into());
        let signing =
            jsonwebtoken::EncodingKey::from_ed_pem(signing.serialize_pem().as_bytes()).unwrap();
        let token = Arc::new(
            jsonwebtoken::encode(
                &header,
                &serde_json::json!({"iss":issuer,"aud":"https://resource.fixture/","exp":expiration,"sub":"fixture-worker","roles":["openshell-admin","openshell-user"]}),
                &signing,
            )
            .unwrap(),
        );
        let state = IssuerState {
            issuer: issuer.clone(),
            requests: requests.clone(),
            token: token.clone(),
            lifetime: lifetime.clone(),
            client_key,
            client_key_id: client_key_id.clone(),
            replay: Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::new())),
        };
        let app = Router::new()
            .route("/.well-known/openid-configuration", get(|State(state): State<IssuerState>| async move {
                Json(serde_json::json!({"issuer":state.issuer,"token_endpoint":state.issuer.as_url().join("token").unwrap(),"token_endpoint_auth_methods_supported":[veoveo_gateway_contract::OAuthClientAuthMethod::PrivateKeyJwt],"jwks_uri":state.issuer.as_url().join("jwks").unwrap()}))
            }))
            .route("/jwks", get(move || { let jwks = jwks.clone(); async move { Json(jwks) } }))
            .route("/client-jwks", get(move || { let jwk = client_jwk.clone(); async move { Json(serde_json::json!({"keys":[jwk]})) } }))
            .route("/token", post(token_endpoint))
            .with_state(state);
        let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem(
            server_cert.as_bytes().to_vec(),
            server_key.as_bytes().to_vec(),
        )
        .await
        .unwrap();
        let handle = axum_server::Handle::new();
        let server_handle = handle.clone();
        let task = tokio::spawn(async move {
            axum_server::from_tcp_rustls(listener, tls_config)
                .unwrap()
                .handle(server_handle)
                .serve(app.into_make_service())
                .await
                .unwrap();
        });
        let config = WorkerOAuthConfig::new(WorkerOAuthFields {
            issuer,
            resource: "https://resource.fixture/".parse().unwrap(),
            client_id: "fixture-worker".parse().unwrap(),
            private_key_file,
            key_id: client_key_id,
            token_endpoint_auth_method:
                veoveo_gateway_contract::OAuthClientAuthMethod::PrivateKeyJwt,
            scopes: ["https://resource.fixture/.default".parse().unwrap()].into(),
            ca_file: Some(dir.join("ca.pem")),
        })
        .unwrap();
        Self {
            config,
            requests,
            token,
            lifetime,
            signing,
            private_key_file: dir.join("worker-private-key.pem"),
            handle,
            task: Some(task),
        }
    }
    /// Deliberately exposed test credentials; callers must keep diagnostics static.
    #[allow(dead_code)] // Used by actual native security controls, not mocked transport.
    pub fn probe_token(&self, probe: WorkerTokenProbe) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let issuer = match probe {
            WorkerTokenProbe::WrongIssuer => "https://foreign.fixture/",
            _ => self.config.issuer().as_str(),
        };
        let audience = match probe {
            WorkerTokenProbe::WrongAudience => "foreign-resource",
            _ => self.config.resource().as_str(),
        };
        let expiry = match probe {
            WorkerTokenProbe::Expired => now - 120,
            _ => now + 3600,
        };
        let roles = match probe {
            WorkerTokenProbe::UnauthorizedRoles => vec!["unrecognized-role"],
            _ => vec!["openshell-admin", "openshell-user"],
        };
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::EdDSA);
        header.kid = Some("fixture-worker".into());
        let foreign;
        let signing = if matches!(probe, WorkerTokenProbe::WrongSignature) {
            let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
            foreign =
                jsonwebtoken::EncodingKey::from_ed_pem(key.serialize_pem().as_bytes()).unwrap();
            &foreign
        } else {
            &self.signing
        };
        jsonwebtoken::encode(&header, &serde_json::json!({"iss":issuer,"aud":audience,"exp":expiry,"sub":"fixture-worker","roles":roles}), signing).unwrap()
    }
    #[allow(dead_code)] // Runtime fixture Drop aborts immediately; Host normal completion awaits.
    pub async fn shutdown(&mut self) {
        self.handle
            .graceful_shutdown(Some(std::time::Duration::from_secs(2)));
        if let Some(mut task) = self.task.take()
            && tokio::time::timeout(std::time::Duration::from_secs(3), &mut task)
                .await
                .is_err()
        {
            task.abort();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(1), task).await;
        }
    }
}
impl Drop for TestIssuer {
    fn drop(&mut self) {
        self.handle.shutdown();
        if let Some(task) = self.task.take() {
            task.abort();
        }
        let _ = std::fs::remove_file(&self.private_key_file);
    }
}

#[derive(serde::Deserialize)]
struct ClientAssertionClaims {
    iss: veoveo_types::OAuthClientId,
    sub: veoveo_types::OAuthClientId,
    aud: veoveo_types::HttpsUrl,
    exp: u64,
    iat: u64,
    nbf: u64,
    jti: String,
}
fn admit_client_assertion(state: &IssuerState, input: &Credentials) -> Result<(), ()> {
    if input.grant_type != "client_credentials"
        || input.client_id.as_str() != "fixture-worker"
        || input.client_assertion_type != veoveo_oauth_client::CLIENT_ASSERTION_TYPE
        || input.resource.as_str() != "https://resource.fixture/"
        || input.scope != "https://resource.fixture/.default"
    {
        return Err(());
    }
    let header = jsonwebtoken::decode_header(&input.client_assertion).map_err(|_| ())?;
    if header.alg != jsonwebtoken::Algorithm::RS256
        || header.kid.as_deref() != Some(state.client_key_id.as_str())
    {
        return Err(());
    }
    let endpoint = state.issuer.as_url().join("token").map_err(|_| ())?;
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
    validation.leeway = 0;
    validation.validate_nbf = true;
    validation.set_issuer(&["fixture-worker"]);
    validation.set_audience(&[endpoint.as_str()]);
    validation.set_required_spec_claims(&["iss", "sub", "aud", "exp", "iat", "nbf", "jti"]);
    let claims = jsonwebtoken::decode::<ClientAssertionClaims>(
        &input.client_assertion,
        &state.client_key,
        &validation,
    )
    .map_err(|_| ())?
    .claims;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ())?
        .as_secs();
    if claims.iss != input.client_id
        || claims.sub != input.client_id
        || claims.aud.as_url() != &endpoint
        || claims.iat > now
        || claims.nbf != claims.iat
        || claims.exp <= now
        || claims.exp.checked_sub(claims.iat).is_none_or(|lifetime| {
            lifetime == 0 || lifetime > veoveo_oauth_client::ASSERTION_LIFETIME_SECONDS
        })
        || uuid::Uuid::parse_str(&claims.jti).is_err()
    {
        return Err(());
    }
    let mut replay = state.replay.lock().map_err(|_| ())?;
    replay.retain(|_, expiry| *expiry > now);
    if replay.len() >= 1024 || replay.contains_key(&claims.jti) {
        return Err(());
    }
    replay.insert(claims.jti, claims.exp);
    Ok(())
}
async fn token_endpoint(
    State(state): State<IssuerState>,
    Form(input): Form<Credentials>,
) -> (axum::http::StatusCode, Json<serde_json::Value>) {
    if admit_client_assertion(&state, &input).is_err() {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error":"invalid_client"})),
        );
    }
    state.requests.fetch_add(1, Ordering::SeqCst);
    (
        axum::http::StatusCode::OK,
        Json(
            serde_json::json!({"access_token":state.token.as_str(),"token_type":"Bearer","expires_in":state.lifetime.load(Ordering::SeqCst)}),
        ),
    )
}

#[cfg(test)]
mod client_assertion_tests {
    use super::*;
    use rsa::pkcs8::EncodePrivateKey;
    use veoveo_oauth_client::{ClientAssertionKeyId, ClientAssertionSigner};

    #[test]
    fn issuer_admits_signed_client_and_refuses_replay_and_claim_or_key_defects() {
        let key_id = ClientAssertionKeyId::parse("private-fixture-key").unwrap();
        let key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let pem = key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).unwrap();
        let signer = ClientAssertionSigner::from_rsa_pem(key_id.clone(), pem.as_bytes()).unwrap();
        let state = IssuerState {
            issuer: veoveo_types::HttpsUrl::parse("https://issuer.fixture/").unwrap(),
            requests: Arc::default(),
            token: Arc::new(String::new()),
            lifetime: Arc::default(),
            client_key: jsonwebtoken::DecodingKey::from_jwk(&signer.public_jwk().unwrap()).unwrap(),
            client_key_id: key_id.clone(),
            replay: Arc::default(),
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let client = veoveo_types::OAuthClientId::parse("fixture-worker").unwrap();
        let credentials = |signer: &ClientAssertionSigner,
                           client: &veoveo_types::OAuthClientId,
                           audience: &str,
                           issued,
                           expiry| Credentials {
            grant_type: "client_credentials".into(),
            client_id: veoveo_types::OAuthClientId::parse("fixture-worker").unwrap(),
            client_assertion_type: veoveo_oauth_client::CLIENT_ASSERTION_TYPE.into(),
            client_assertion: signer
                .diagnostic_assertion(
                    client,
                    audience,
                    issued,
                    expiry,
                    &uuid::Uuid::new_v4().to_string(),
                )
                .unwrap()
                .expose_secret()
                .into(),
            resource: veoveo_gateway_contract::ProtectedResourceId::parse(
                "https://resource.fixture/",
            )
            .unwrap(),
            scope: "https://resource.fixture/.default".into(),
        };
        let valid = credentials(
            &signer,
            &client,
            "https://issuer.fixture/token",
            now,
            now + 60,
        );
        assert!(admit_client_assertion(&state, &valid).is_ok());
        assert!(
            admit_client_assertion(&state, &valid).is_err(),
            "replay must be refused"
        );
        for (client, audience, issued, expiry) in [
            (client.clone(), "https://other.fixture/token", now, now + 60),
            (
                veoveo_types::OAuthClientId::parse("other-client").unwrap(),
                "https://issuer.fixture/token",
                now,
                now + 60,
            ),
            (
                client.clone(),
                "https://issuer.fixture/token",
                now - 120,
                now - 60,
            ),
            (
                client.clone(),
                "https://issuer.fixture/token",
                now,
                now + 61,
            ),
        ] {
            assert!(
                admit_client_assertion(
                    &state,
                    &credentials(&signer, &client, audience, issued, expiry)
                )
                .is_err()
            );
        }
        let wrong_key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let wrong_pem = wrong_key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).unwrap();
        let wrong_signer =
            ClientAssertionSigner::from_rsa_pem(key_id, wrong_pem.as_bytes()).unwrap();
        assert!(
            admit_client_assertion(
                &state,
                &credentials(
                    &wrong_signer,
                    &client,
                    "https://issuer.fixture/token",
                    now,
                    now + 60
                )
            )
            .is_err()
        );
        let wrong_kid = ClientAssertionSigner::from_rsa_pem(
            ClientAssertionKeyId::parse("unknown-key").unwrap(),
            pem.as_bytes(),
        )
        .unwrap();
        assert!(
            admit_client_assertion(
                &state,
                &credentials(
                    &wrong_kid,
                    &client,
                    "https://issuer.fixture/token",
                    now,
                    now + 60
                )
            )
            .is_err()
        );
        let mut wrong_resource = credentials(
            &signer,
            &client,
            "https://issuer.fixture/token",
            now,
            now + 60,
        );
        wrong_resource.resource =
            veoveo_gateway_contract::ProtectedResourceId::parse("https://other-resource.fixture/")
                .unwrap();
        assert!(admit_client_assertion(&state, &wrong_resource).is_err());
    }
}
