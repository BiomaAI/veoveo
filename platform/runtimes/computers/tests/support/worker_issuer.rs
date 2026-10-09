//! HTTPS OAuth fixtures establish worker transport, not native provider policy.
use super::{WorkerOAuthConfig, WorkerOAuthFields, WorkerTokenAuthentication};
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
    secret: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Credentials {
    grant_type: String,
    client_id: veoveo_types::OAuthClientId,
    client_secret: String,
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
    secret_file: PathBuf,
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
        use std::os::unix::fs::PermissionsExt;
        let secret = dir.join("worker-secret");
        let credential = format!("fixture-+&%=worker-{}", uuid::Uuid::now_v7());
        std::fs::write(&secret, &credential).unwrap();
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
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
                &serde_json::json!({"iss":issuer,"aud":"fixture-resource","exp":expiration,"sub":"fixture-worker","roles":["openshell-admin","openshell-user"]}),
                &signing,
            )
            .unwrap(),
        );
        let state = IssuerState {
            issuer: issuer.clone(),
            requests: requests.clone(),
            token: token.clone(),
            lifetime: lifetime.clone(),
            secret: credential,
        };
        let app = Router::new()
            .route("/.well-known/openid-configuration", get(|State(state): State<IssuerState>| async move {
                Json(serde_json::json!({"issuer":state.issuer,"token_endpoint":state.issuer.as_url().join("token").unwrap(),"token_endpoint_auth_methods_supported":["client_secret_post"],"jwks_uri":state.issuer.as_url().join("jwks").unwrap()}))
            }))
            .route("/jwks", get(move || { let jwks = jwks.clone(); async move { Json(jwks) } }))
            .route("/token", post(|State(state): State<IssuerState>, Form(input): Form<Credentials>| async move {
                assert_eq!(input.grant_type, "client_credentials");
                assert_eq!(input.client_id.as_str(), "fixture-worker");
                assert!(input.client_secret == state.secret, "fixture credential refused");
                assert_eq!(input.scope, "https://resource.fixture/.default");
                state.requests.fetch_add(1, Ordering::SeqCst);
                Json(serde_json::json!({"access_token":state.token.as_str(),"token_type":"Bearer","expires_in":state.lifetime.load(Ordering::SeqCst)}))
            }))
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
            audience: "fixture-resource".into(),
            client_id: "fixture-worker".parse().unwrap(),
            client_secret_file: secret,
            token_endpoint_auth_method: WorkerTokenAuthentication::ClientSecretPost,
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
            secret_file: dir.join("worker-secret"),
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
            _ => self.config.audience(),
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
        let _ = std::fs::remove_file(&self.secret_file);
    }
}
