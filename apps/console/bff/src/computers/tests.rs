use super::*;
use crate::{
    api,
    config::Config,
    session::{ConsoleSession, SESSION_AAD, SESSION_COOKIE, SessionCipher},
};
use axum::{
    body::{Body, to_bytes},
    extract::Request,
    middleware,
    routing::any,
};
use chrono::Utc;
use std::{collections::BTreeSet, sync::Mutex, time::Duration};
use tokio::task::JoinHandle;
use tower::ServiceExt;
use uuid::Uuid;
use veoveo_mcp_contract::ScopeName;

struct Observed {
    path: String,
    headers: HeaderMap,
}
struct Fixture {
    state: AppState,
    router: Router,
    observed: Arc<Mutex<Vec<Observed>>>,
    jobs: Vec<JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.state.computers.stop.cancel();
        for job in &self.jobs {
            job.abort();
        }
    }
}
impl Fixture {
    async fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let capture = observed.clone();
        let upstream = terminal::upstream(observed.clone()).fallback(any(move |request: Request| {
            let capture = capture.clone();
            async move {
                let path = request.uri().path().to_owned();
                capture.lock().unwrap().push(Observed { path: path.clone(), headers: request.headers().clone() });
                if path == "/oauth/token" {
                    return Json(serde_json::json!({"access_token":"rotated-fixture-access", "token_type":"Bearer", "expires_in":300, "refresh_token":"rotated-fixture-refresh", "refresh_token_expires_in":3600, "scope":"admin:manage"})).into_response();
                }
                if path == "/console-api/admin/session" {
                    return ([(header::SET_COOKIE, "upstream=forbidden")], Json(serde_json::json!({
                        "profile":"admin", "canReadInstallation":false,
                        "installation":{"name":"Veoveo","productLabel":"Workspace","version":"fixture","offlineMode":false,"generatedAt":Utc::now()},
                        "session":{"displayName":"Alice","principalId":"https://test#alice","actorId":"https://test#alice","tenantId":"test","tenantName":"Test","workContext":"work","workContextTitle":"Work","membership":"contributor","invocationMode":"direct","availableTenants":[{"id":"test","name":"Test"}]}
                    }))).into_response();
                }
                if path.ends_with("/start") { return (StatusCode::TEMPORARY_REDIRECT, [(header::LOCATION, "/must-not-follow")]).into_response(); }
                if path.ends_with("/stop") { return fault(StatusCode::SERVICE_UNAVAILABLE); }
                if path.ends_with("/terminal-ticket") {
                    let id = path.split('/').nth(3).unwrap().parse::<Uuid>().unwrap();
                    let body = veoveo_computers_contract::TerminalTicket { computer_id: id, token: veoveo_computers_contract::TerminalToken::new("only-in-body".into()), expires_at: Utc::now(), endpoint: "https://foreign.invalid/?token=never".into() };
                    return (StatusCode::CREATED, [(header::SET_COOKIE, "foreign=value")], Json(body)).into_response();
                }
                (StatusCode::OK, Json(serde_json::json!({"fixture":true}))).into_response()
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let job = tokio::spawn(async move {
            axum::serve(listener, upstream).await.unwrap();
        });
        let config = Arc::new(Config::for_test(base.parse().unwrap()));
        let trust = OutboundTrust::default();
        let http = trust
            .client_builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let state = AppState {
            config: config.clone(),
            http: http.clone(),
            stream_http: http.clone(),
            live_http: http,
            cluster: None,
            sessions: SessionCipher::new(config.session_key()).unwrap(),
            mcp: Arc::new(crate::mcp_client::AuthScopedMcpClientPool::new(&trust).unwrap()),
            app_tasks: crate::apps::AppTaskRegistry::default(),
            computers: Transport::new(&trust).unwrap(),
        };
        let router = super::router()
            .route(
                "/console/api/session",
                axum::routing::get(crate::bootstrap::session),
            )
            .with_state(state.clone())
            .layer(middleware::from_fn_with_state(
                state.clone(),
                api::enforce_csrf,
            ));
        Self {
            state,
            router,
            observed,
            jobs: vec![job],
        }
    }
    fn cookie(&self, refresh: bool) -> String {
        let now = Utc::now().timestamp();
        let session = ConsoleSession {
            access_token: "fixture-access".into(),
            access_expires_at: now + if refresh { 1 } else { 300 },
            refresh_token: "fixture-refresh".into(),
            refresh_expires_at: now + 3600,
            granted_scopes: BTreeSet::from([ScopeName::new("admin:manage").unwrap()]),
            csrf_token: "fixture-csrf".into(),
        };
        format!(
            "{SESSION_COOKIE}={}",
            self.state.sessions.seal(&session, SESSION_AAD).unwrap()
        )
    }
    async fn call(&self, request: Request) -> Response {
        tokio::time::timeout(
            Duration::from_secs(10),
            self.router.clone().oneshot(request),
        )
        .await
        .unwrap()
        .unwrap()
    }
    fn mutation(&self, path: &str, refresh: bool) -> axum::http::request::Builder {
        Request::builder()
            .method("POST")
            .uri(path)
            .header(header::COOKIE, self.cookie(refresh))
            .header(api::CSRF_HEADER, "fixture-csrf")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, self.state.config.public_origin())
    }
}

#[tokio::test]
async fn session_bootstrap_uses_cookie_authority_and_preserves_rotation_without_admin_inventory() {
    let fixture = Fixture::new().await;
    let anonymous = fixture
        .call(
            Request::builder()
                .uri("/console/api/session")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
    let invalid = fixture
        .call(
            Request::builder()
                .uri("/console/api/session?profile=foreign")
                .header(header::COOKIE, fixture.cookie(true))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    assert!(fixture.observed.lock().unwrap().is_empty());
    let response = fixture
        .call(
            Request::builder()
                .uri("/console/api/session")
                .header(header::COOKIE, fixture.cookie(true))
                .header(header::AUTHORIZATION, "Bearer forged")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key(api::CSRF_HEADER));
    assert_eq!(
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .count(),
        1
    );
    assert!(
        !response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("upstream=")
    );
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let value: veoveo_mcp_contract::ConsoleBootstrap =
        serde_json::from_slice(&to_bytes(response.into_body(), 256 * 1024).await.unwrap()).unwrap();
    assert!(!value.can_read_installation);
    let observed = fixture.observed.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert_eq!(observed[1].path, "/console-api/admin/session");
    assert_eq!(
        observed[1].headers[header::AUTHORIZATION],
        "Bearer rotated-fixture-access"
    );
    assert!(!observed[1].headers.contains_key(header::COOKIE));
}

#[tokio::test]
async fn anonymous_and_csrf_failures_never_contact_gateway() {
    let fixture = Fixture::new().await;
    let response = fixture
        .call(
            Request::builder()
                .uri("/console/api/computers")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    for csrf in [None, Some("foreign")] {
        let mut request = Request::builder()
            .method("POST")
            .uri("/console/api/computers")
            .header(header::COOKIE, fixture.cookie(false));
        if let Some(csrf) = csrf {
            request = request.header(api::CSRF_HEADER, csrf);
        }
        assert_eq!(
            fixture
                .call(request.body(Body::from("{}")).unwrap())
                .await
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    assert!(fixture.observed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn ticket_uses_cookie_profile_and_rewrites_only_owned_endpoint() {
    let fixture = Fixture::new().await;
    let id = Uuid::new_v4();
    let path = format!("/console/api/computers/{id}/terminal-ticket");
    let response = fixture
        .call(
            fixture
                .mutation(&path, false)
                .header(header::AUTHORIZATION, "Bearer caller-forgery")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert!(!response.headers().contains_key(header::SET_COOKIE));
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let ticket: veoveo_computers_contract::TerminalTicket = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        ticket.endpoint,
        format!("/console/api/computers/{id}/terminal")
    );
    let observed = fixture.observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0].path,
        format!("/computers/admin/{id}/terminal-ticket")
    );
    assert_eq!(
        observed[0].headers[header::AUTHORIZATION],
        "Bearer fixture-access"
    );
    assert_eq!(
        observed[0].headers[header::HOST],
        fixture.state.config.gateway_host()
    );
    assert_eq!(
        observed[0].headers[header::ORIGIN],
        fixture.state.config.public_origin()
    );
    assert!(!observed[0].headers.contains_key(header::COOKIE));
}

#[tokio::test]
async fn operation_status_is_a_cookie_scoped_read_and_rejects_extra_query_authority() {
    let fixture = Fixture::new().await;
    let computer = Uuid::new_v4();
    let operation = Uuid::new_v4();
    let path = format!("/console/api/computers/{computer}/operations/{operation}");
    let response = fixture
        .call(
            Request::builder()
                .uri(&path)
                .header(header::COOKIE, fixture.cookie(false))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    {
        let observed = fixture.observed.lock().unwrap();
        assert_eq!(observed.len(), 1);
        assert_eq!(
            observed[0].path,
            format!("/computers/admin/{computer}/operations/{operation}")
        );
    }
    let response = fixture
        .call(
            Request::builder()
                .uri(format!("{path}?after={computer}"))
                .header(header::COOKIE, fixture.cookie(false))
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(fixture.observed.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn access_revocation_keeps_csrf_even_when_it_only_reduces_authority() {
    let fixture = Fixture::new().await;
    let computer = Uuid::new_v4();
    let grant = Uuid::new_v4();
    let path = format!("/console/api/computers/{computer}/access/{grant}/revoke");
    let denied = fixture
        .call(
            Request::builder()
                .method("POST")
                .uri(&path)
                .header(header::COOKIE, fixture.cookie(false))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert!(fixture.observed.lock().unwrap().is_empty());
    let accepted = fixture
        .call(
            fixture
                .mutation(&path, false)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert_eq!(accepted.status(), StatusCode::OK);
    {
        let observed = fixture.observed.lock().unwrap();
        assert_eq!(observed.len(), 1);
        assert_eq!(
            observed[0].path,
            format!("/computers/admin/{computer}/access/{grant}/revoke")
        );
    }
    let query = fixture
        .call(
            fixture
                .mutation(&format!("{path}?owner=bob"), false)
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
    assert_eq!(query.status(), StatusCode::BAD_REQUEST);
    assert_eq!(fixture.observed.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn origin_queries_and_body_limits_fail_before_refresh() {
    let fixture = Fixture::new().await;
    let path = format!("/console/api/computers/{}/terminal-ticket", Uuid::new_v4());
    let mut request = fixture
        .mutation(&path, true)
        .body(Body::from("{}"))
        .unwrap();
    request.headers_mut().append(
        header::ORIGIN,
        HeaderValue::from_str(&fixture.state.config.public_origin()).unwrap(),
    );
    assert_eq!(fixture.call(request).await.status(), StatusCode::FORBIDDEN);
    let mut request = fixture
        .mutation(&path, true)
        .body(Body::from("{}"))
        .unwrap();
    request.headers_mut().remove(header::ORIGIN);
    assert_eq!(fixture.call(request).await.status(), StatusCode::FORBIDDEN);
    for path in [
        format!("{path}?token=forged"),
        "/console/api/computers?owner=forged".into(),
    ] {
        assert_eq!(
            fixture
                .call(
                    fixture
                        .mutation(&path, true)
                        .body(Body::from("{}"))
                        .unwrap()
                )
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        fixture
            .call(
                fixture
                    .mutation("/console/api/computers", true)
                    .body(Body::from(vec![b'x'; 65537]))
                    .unwrap()
            )
            .await
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert!(fixture.observed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn completed_refresh_is_returned_on_upstream_failure_and_redirects_are_not_followed() {
    let fixture = Fixture::new().await;
    for action in ["start", "stop"] {
        let path = format!("/console/api/computers/{}/{action}", Uuid::new_v4());
        let response = fixture
            .call(
                fixture
                    .mutation(&path, true)
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[api::CSRF_HEADER], "fixture-csrf");
        let cookie = response.headers()[header::SET_COOKIE].to_str().unwrap();
        let encoded = cookie
            .split(';')
            .next()
            .unwrap()
            .strip_prefix(&format!("{SESSION_COOKIE}="))
            .unwrap();
        let renewed: ConsoleSession = fixture.state.sessions.open(encoded, SESSION_AAD).unwrap();
        assert_eq!(renewed.access_token, "rotated-fixture-access");
    }
    let observed = fixture.observed.lock().unwrap();
    assert_eq!(observed.len(), 4);
    assert!(
        !observed
            .iter()
            .any(|request| request.path == "/must-not-follow")
    );
    assert_eq!(
        observed[1].headers[header::AUTHORIZATION],
        "Bearer rotated-fixture-access"
    );
}

#[path = "tests_terminal.rs"]
mod terminal;
