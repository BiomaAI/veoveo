//! Isolated OAuth issuer fixture; the production login uses the pinned SDK client.
mod connection;
use axum::{
    Json, Router,
    extract::{Form, Query, State},
    http::StatusCode,
    response::Redirect,
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
pub use connection::StalledConnection;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{net::TcpListener, sync::oneshot, task::JoinHandle};

pub const TOKEN: &str = "fixture-access-token-never-print";
pub const SECRET_MARKER: &str = "fixture-remote-secret-never-print";
pub const REFRESH_TOKEN: &str = "fixture-private-refresh-never-print";
pub const VENDOR_SECRET: &str = "fixture-private-vendor-field-never-print";
pub const SCOPE: &str = "knowledge:read";
pub const CLIENT: &str = "fixture-public";

#[derive(Clone, Default)]
pub struct LogCapture(Arc<Mutex<Vec<u8>>>);

impl LogCapture {
    pub fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

impl std::io::Write for LogCapture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[derive(Clone)]
struct Issuer {
    base: String,
    advertised_scopes: Vec<String>,
    granted_scope: String,
    deny: bool,
    challenge: Arc<Mutex<Option<String>>>,
    tokens: Arc<AtomicUsize>,
}

#[derive(Clone)]
pub struct Context {
    pub base: String,
    pub tokens: Arc<AtomicUsize>,
    pub directory: PathBuf,
}

pub struct Fixture {
    pub context: Context,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<std::io::Result<()>>>,
}

impl Fixture {
    pub async fn start(advertise_offline: bool, granted_scope: &str, deny: bool) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("veoveo-oauth-login-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let tokens = Arc::new(AtomicUsize::new(0));
        let challenge = Arc::new(Mutex::new(None));
        let mut advertised_scopes = vec![SCOPE.to_owned()];
        if advertise_offline {
            advertised_scopes.push("offline_access".into());
        }
        let issuer = Issuer {
            base: base.clone(),
            advertised_scopes,
            granted_scope: granted_scope.into(),
            deny,
            challenge: challenge.clone(),
            tokens: tokens.clone(),
        };
        let app = Router::new()
            .route("/.well-known/oauth-protected-resource", get(resource))
            .route("/.well-known/oauth-protected-resource/mcp", get(resource))
            .route("/.well-known/oauth-authorization-server", get(metadata))
            .route("/.well-known/openid-configuration", get(metadata))
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .with_state(issuer);
        let (stop, stopped) = oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .await
        });
        Self {
            context: Context {
                base,
                tokens,
                directory,
            },
            stop: Some(stop),
            task: Some(task),
        }
    }
}

impl Context {
    pub fn output(&self) -> PathBuf {
        self.directory.join("access-token")
    }

    pub async fn admit_authorization(&self, authorization: &url::Url) -> url::Url {
        let query: BTreeMap<_, _> = authorization
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        assert_eq!(
            authorization.as_str().split('?').next().unwrap(),
            format!("{}/authorize", self.base)
        );
        assert_eq!(query.get("client_id").unwrap(), CLIENT);
        assert_eq!(query.get("code_challenge_method").unwrap(), "S256");
        assert_eq!(query.get("scope").unwrap(), SCOPE);
        assert_eq!(
            query.get("resource").unwrap(),
            &format!("{}/mcp", self.base)
        );
        let response = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap()
            .get(authorization.clone())
            .send()
            .await
            .unwrap();
        assert!(response.status().is_redirection());
        url::Url::parse(
            response
                .headers()
                .get(reqwest::header::LOCATION)
                .unwrap()
                .to_str()
                .unwrap(),
        )
        .unwrap()
    }
}

impl Fixture {
    pub async fn close(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.as_mut() {
            match tokio::time::timeout(Duration::from_secs(3), &mut *task).await {
                Ok(result) => {
                    result.unwrap().unwrap();
                }
                Err(_) => {
                    task.abort();
                    let _ = task.await;
                    panic!("OAuth issuer fixture did not drain");
                }
            }
        }
        self.task = None;
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
        let _ = std::fs::remove_dir_all(&self.context.directory);
    }
}

async fn resource(State(s): State<Issuer>) -> Json<Value> {
    Json(
        json!({"resource":format!("{}/mcp",s.base),"authorization_servers":[format!("{}/",s.base)],"scopes_supported":[SCOPE],"bearer_methods_supported":["header"]}),
    )
}

async fn metadata(State(s): State<Issuer>) -> Json<Value> {
    Json(
        json!({"issuer":format!("{}/",s.base),"authorization_endpoint":format!("{}/authorize",s.base),"token_endpoint":format!("{}/token",s.base),"response_types_supported":["code"],"grant_types_supported":["authorization_code"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"],"authorization_response_iss_parameter_supported":true,"scopes_supported":s.advertised_scopes}),
    )
}

async fn authorize(
    State(s): State<Issuer>,
    Query(query): Query<BTreeMap<String, String>>,
) -> Redirect {
    assert_eq!(query.get("client_id").unwrap(), CLIENT);
    assert_eq!(query.get("code_challenge_method").unwrap(), "S256");
    assert_eq!(query.get("scope").unwrap(), SCOPE);
    *s.challenge.lock().unwrap() = Some(query.get("code_challenge").unwrap().clone());
    let mut callback = url::Url::parse(query.get("redirect_uri").unwrap()).unwrap();
    callback
        .query_pairs_mut()
        .append_pair("code", "fixture-code")
        .append_pair("state", query.get("state").unwrap())
        .append_pair("iss", &format!("{}/", s.base));
    Redirect::to(callback.as_str())
}

async fn token(
    State(s): State<Issuer>,
    Form(form): Form<BTreeMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    s.tokens.fetch_add(1, Ordering::SeqCst);
    let challenge = form
        .get("code_verifier")
        .map(|verifier| URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())));
    let valid = challenge.is_some()
        && challenge == *s.challenge.lock().unwrap()
        && form.get("client_id").is_some_and(|id| id == CLIENT)
        && form
            .get("grant_type")
            .is_some_and(|grant| grant == "authorization_code")
        && form.get("code").is_some_and(|code| code == "fixture-code")
        && form
            .get("resource")
            .is_some_and(|resource| resource == &format!("{}/mcp", s.base))
        && !form.contains_key("client_secret");
    if !valid || s.deny {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":"access_denied","error_description":SECRET_MARKER})),
        );
    }
    (
        StatusCode::OK,
        Json(
            json!({"access_token":TOKEN,"token_type":"Bearer","expires_in":300,"scope":s.granted_scope,"refresh_token":REFRESH_TOKEN,"vendor_private":VENDOR_SECRET}),
        ),
    )
}
