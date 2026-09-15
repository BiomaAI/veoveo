use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::Query,
    http::{HeaderMap, Request, StatusCode},
    middleware,
    routing::{get, post},
};
use chrono::Utc;
use serde_json::json;
use tower::ServiceExt;

use crate::{
    AppState, api,
    config::Config,
    session::{BrowserSession, SESSION_AAD, SessionCipher},
};

struct Edge {
    app: Router,
    cookie: String,
    server: tokio::task::JoinHandle<()>,
}

impl Drop for Edge {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl Edge {
    async fn new(upstream: Router) -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = url::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, upstream).await.unwrap();
        });
        let config = Arc::new(Config::workspace_for_test(url));
        let sessions = SessionCipher::new(config.session_key())
            .unwrap()
            .for_app(config.app());
        let now = Utc::now().timestamp();
        let cookie = sessions
            .seal(
                &BrowserSession {
                    access_token: "cookie-access".into(),
                    access_expires_at: now + 300,
                    refresh_token: "cookie-refresh".into(),
                    refresh_expires_at: now + 3600,
                    granted_scopes: config.oauth_scopes().clone(),
                    csrf_token: "csrf".into(),
                },
                SESSION_AAD,
            )
            .unwrap();
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let state = AppState {
            config,
            http: client.clone(),
            stream_http: client.clone(),
            live_http: client,
            cluster: None,
            sessions,
            mcp: Arc::new(
                crate::mcp_client::AuthScopedMcpClientPool::new(&Default::default()).unwrap(),
            ),
            app_tasks: crate::apps::AppTaskRegistry::default(),
            computers: crate::computers::Transport::new(&Default::default()).unwrap(),
        };
        let app = super::router()
            .route("/workspace/auth/login", get(crate::oauth::login))
            .with_state(state.clone())
            .layer(middleware::from_fn_with_state(state, api::enforce_csrf));
        Self {
            app,
            cookie: format!("veoveo_workspace={cookie}"),
            server,
        }
    }

    async fn request(
        &self,
        method: &str,
        path: &str,
        authenticated: bool,
        csrf: bool,
        body: &str,
    ) -> axum::response::Response {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .header("authorization", "Bearer forged-browser-token")
            .header("host", "untrusted.invalid");
        if authenticated {
            request = request.header("cookie", &self.cookie);
        }
        if csrf {
            request = request.header(api::CSRF_HEADER, "csrf");
        }
        self.app
            .clone()
            .oneshot(request.body(Body::from(body.to_owned())).unwrap())
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn workspace_login_uses_its_own_client_callback_scopes_and_pending_cookie() {
    let upstream = Router::new().route("/.well-known/oauth-protected-resource/mcp/workspace", get(|headers: HeaderMap| async move {
        Json(json!({"resource": format!("http://{}/mcp/workspace", headers["host"].to_str().unwrap()), "scopes_supported":["operator:use","artifact:upload"]}))
    }));
    let edge = Edge::new(upstream).await;
    let response = edge
        .request(
            "GET",
            "/workspace/auth/login?return_to=%2Fworkspace%2F%3Fchat%3Done",
            false,
            false,
            "",
        )
        .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let url = url::Url::parse(response.headers()["location"].to_str().unwrap()).unwrap();
    let query: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(query["client_id"], "workspace");
    assert_eq!(query["scope"], "artifact:upload operator:use");
    assert!(query["resource"].ends_with("/mcp/workspace"));
    assert!(query["redirect_uri"].ends_with("/workspace/auth/callback"));
    assert_eq!(query["code_challenge_method"], "S256");
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.starts_with("veoveo_workspace_authorization="));
    assert!(cookie.contains("HttpOnly"));
    assert!(!cookie.contains("veoveo_console"));
}

#[tokio::test]
async fn cookie_identity_and_csrf_are_required_and_browser_headers_do_not_reach_gateway() {
    let requests = Arc::new(AtomicUsize::new(0));
    let observed = requests.clone();
    let id = uuid::Uuid::now_v7();
    let upstream = Router::new().route(
        "/workspace-api/workspace/chats",
        post(move |headers: HeaderMap| {
            let observed = observed.clone();
            async move {
                assert_eq!(headers["authorization"], "Bearer cookie-access");
                assert!(!headers.contains_key("cookie"));
                assert_ne!(headers["host"], "untrusted.invalid");
                observed.fetch_add(1, Ordering::SeqCst);
                Json(json!({"id":id,"title":"Shared","owner":id,"archived":false,
                "membersCanInvite":false,"sequence":1,"revision":0,"updatedAt":Utc::now()}))
            }
        }),
    );
    let edge = Edge::new(upstream).await;
    let body = json!({"id":id,"title":"Shared"}).to_string();
    assert_eq!(
        edge.request("POST", "/workspace/api/chats", false, true, &body)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        edge.request("POST", "/workspace/api/chats", true, false, &body)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    let response = edge
        .request("POST", "/workspace/api/chats", true, true, &body)
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()[api::CSRF_HEADER], "csrf");
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("cookie-access"));
    assert_eq!(requests.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn search_is_encoded_and_oversized_or_invalid_upstream_payloads_fail_closed() {
    let upstream = Router::new().route("/workspace-api/workspace/people", get(|Query(query): Query<std::collections::BTreeMap<String, String>>| async move {
        assert_eq!(query.len(), 1);
        match query["q"].as_str() {
            "Bob&destination=https://untrusted.invalid" => Json(json!([])),
            "invalid" => Json(json!({"private":"unexpected"})),
            "oversized" => Json(json!([{"id":uuid::Uuid::now_v7(),"displayName":"x".repeat(4 * 1024 * 1024)}])),
            _ => panic!("unexpected fixture search"),
        }
    }));
    let edge = Edge::new(upstream).await;
    assert_eq!(
        edge.request(
            "GET",
            "/workspace/api/people?q=Bob%26destination%3Dhttps%3A%2F%2Funtrusted.invalid",
            true,
            false,
            ""
        )
        .await
        .status(),
        StatusCode::OK
    );
    for query in ["invalid", "oversized"] {
        let response = edge
            .request(
                "GET",
                &format!("/workspace/api/people?q={query}"),
                true,
                false,
                "",
            )
            .await;
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(response.headers()[api::CSRF_HEADER], "csrf");
    }
}

#[test]
fn workspace_oauth_return_path_preserves_chat_and_rejects_external_redirects() {
    use crate::session::BrowserReturnPath;
    assert_eq!(
        BrowserReturnPath::from_untrusted(Some("/workspace/?chat=one")).as_str(),
        "/workspace/?chat=one"
    );
    assert_eq!(
        BrowserReturnPath::from_untrusted(Some("//untrusted.invalid/workspace/")).as_str(),
        "/console/"
    );
}

#[tokio::test]
async fn chat_stream_uses_cookie_authority_and_rejects_query_credentials() {
    let chat = uuid::Uuid::now_v7();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let upstream = Router::new().route(
        &format!("/workspace-api/workspace/chats/{chat}/events"),
        get(move |headers: HeaderMap| {
            count.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(headers["authorization"], "Bearer cookie-access");
                assert!(headers.get("cookie").is_none());
                assert_ne!(headers["host"], "untrusted.invalid");
                (
                    [("content-type", "text/event-stream")],
                    "event: change\nid: 8\ndata: {\"sequence\":8}\n\n",
                )
            }
        }),
    );
    let edge = Edge::new(upstream).await;
    let path = format!("/workspace/api/chats/{chat}/events");
    assert_eq!(
        edge.request("GET", &path, false, false, "").await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        edge.request("GET", &format!("{path}?token=forged"), true, false, "")
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let response = edge.request("GET", &path, true, false, "").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[api::CSRF_HEADER], "csrf");
    assert_eq!(response.headers()["cache-control"], "no-store");
    let bytes = to_bytes(response.into_body(), 1024).await.unwrap();
    assert!(
        std::str::from_utf8(&bytes)
            .unwrap()
            .contains("\"sequence\":8")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn agent_run_admission_keeps_cookie_identity_and_rejects_browser_model_configuration() {
    let chat = uuid::Uuid::now_v7();
    let agent = uuid::Uuid::now_v7();
    let trigger = uuid::Uuid::now_v7();
    let run = uuid::Uuid::now_v7();
    let initiator = uuid::Uuid::now_v7();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let upstream = Router::new().route(&format!("/workspace-api/workspace/chats/{chat}/runs"), post(move |headers: HeaderMap, Json(body): Json<serde_json::Value>| {
        let observed = observed.clone();
        async move {
            observed.fetch_add(1, Ordering::SeqCst);
            assert_eq!(headers["authorization"], "Bearer cookie-access");
            assert_eq!(body, serde_json::json!({"agent":agent,"trigger":trigger}));
            Json(serde_json::json!({"id":run,"agent":agent,"initiator":initiator,"trigger":trigger,"state":"queued","text":"","failure":null,"sequence":4,"updatedSequence":4,"createdAt":"2026-09-15T12:00:00Z"}))
        }
    }));
    let edge = Edge::new(upstream).await;
    let path = format!("/workspace/api/chats/{chat}/runs");
    let body = serde_json::json!({"agent":agent,"trigger":trigger}).to_string();
    assert_eq!(
        edge.request("POST", &path, true, false, &body)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let forged = serde_json::json!({"agent":agent,"trigger":trigger,"modelUrl":"https://untrusted.invalid","initiator":initiator}).to_string();
    assert_eq!(
        edge.request("POST", &path, true, true, &forged)
            .await
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let response = edge.request("POST", &path, true, true, &body).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
