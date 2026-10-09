//! HTTPS OAuth fixtures establish worker transport, not native provider policy.
use super::{WorkerOAuthConfig, WorkerOAuthFields, WorkerTokenAuthentication};
use axum::{
    Form, Json, Router,
    extract::State,
    routing::{get, post},
};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::{
    path::Path,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

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
    task: tokio::task::JoinHandle<()>,
}
impl TestIssuer {
    pub async fn start(dir: &Path, server_cert: &str, server_key: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let secret = dir.join("worker-secret");
        let credential = format!("fixture-+&%=worker-{}", uuid::Uuid::now_v7());
        std::fs::write(&secret, &credential).unwrap();
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let issuer = veoveo_types::HttpsUrl::parse(&format!(
            "https://localhost:{}/",
            listener.local_addr().unwrap().port()
        ))
        .unwrap();
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
        let token = Arc::new(
            jsonwebtoken::encode(
                &header,
                &serde_json::json!({"iss":issuer,"aud":"fixture-resource","exp":expiration,"sub":"fixture-worker","roles":["openshell-admin","openshell-user"]}),
                &jsonwebtoken::EncodingKey::from_ed_pem(signing.serialize_pem().as_bytes()).unwrap(),
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
        let task = tokio::spawn(async move {
            axum_server::from_tcp_rustls(listener, tls_config)
                .unwrap()
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
            task,
        }
    }
}
impl Drop for TestIssuer {
    fn drop(&mut self) {
        self.task.abort();
    }
}
