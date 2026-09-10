use super::*;
use axum::extract::{
    OriginalUri,
    ws::{Message, WebSocketUpgrade},
};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as ClientMessage, client::IntoClientRequest},
};

pub(super) fn upstream(capture: Arc<Mutex<Vec<Observed>>>) -> Router {
    let handler = move |OriginalUri(uri): OriginalUri, headers: HeaderMap, ws: WebSocketUpgrade| {
        capture.lock().unwrap().push(Observed {
            path: uri.path().into(),
            headers,
        });
        async move {
            ws.on_upgrade(|mut socket| async move {
                let ready = veoveo_computers_contract::TerminalReady {
                    version: 2,
                    kind: veoveo_computers_contract::TerminalReadyKind::Ready,
                    expires_at: Utc::now() + chrono::TimeDelta::seconds(20),
                };
                socket
                    .send(Message::Text(serde_json::to_string(&ready).unwrap().into()))
                    .await
                    .unwrap();
                while let Some(Ok(Message::Binary(bytes))) = socket.next().await {
                    if socket.send(Message::Binary(bytes)).await.is_err() {
                        break;
                    }
                }
            })
        }
    };
    Router::new()
        .route("/computers/admin/cli/_ws_tunnel", get(handler.clone()))
        .route("/computers/admin/cli/{id}/_ws_tunnel", get(handler))
}

#[tokio::test]
async fn stock_cli_uses_both_public_routes_without_cookie_authority_or_lease_text() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut fixture = Fixture::new().await;
        let base = terminal::listen(&mut fixture).await;
        let id = Uuid::new_v4();
        let token = format!("vcli1.{}.{}", Uuid::new_v4(), "a".repeat(64));
        for path in ["/_ws_tunnel".into(), format!("/console/computers/{id}/_ws_tunnel")] {
            let mut request = format!("{base}{path}").into_client_request().unwrap();
            request.headers_mut().insert("cf-access-token", token.parse().unwrap());
            request.headers_mut().insert("cf-access-jwt-assertion", token.parse().unwrap());
            request.headers_mut().insert(header::COOKIE, format!("CF_Authorization={token}").parse().unwrap());
            let (mut socket, response) = connect_async(request).await.unwrap();
            assert!(!response.headers().contains_key(header::SET_COOKIE));
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            socket.send(ClientMessage::Binary(b"grpc-fixture".to_vec().into())).await.unwrap();
            assert!(matches!(socket.next().await.unwrap().unwrap(), ClientMessage::Binary(bytes) if bytes == b"grpc-fixture".as_slice()));
            socket.close(None).await.unwrap();
        }
        let observed = fixture.observed.lock().unwrap();
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0].path, "/computers/admin/cli/_ws_tunnel");
        assert_eq!(observed[1].path, format!("/computers/admin/cli/{id}/_ws_tunnel"));
        for request in observed.iter() {
            assert!(request.headers[header::AUTHORIZATION].as_bytes() == format!("Bearer {token}").as_bytes());
            assert!(!request.headers.contains_key(header::COOKIE));
            assert!(!request.headers.contains_key(header::ORIGIN));
            assert!(!request.headers.contains_key("cf-access-token"));
        }
    }).await.unwrap();
}

#[tokio::test]
async fn pairing_posts_require_csrf_and_exact_origin_and_have_fixed_profile_paths() {
    let fixture = Fixture::new().await;
    let id = Uuid::new_v4();
    let pairing = Uuid::new_v4();
    for suffix in [
        "cli-pairings".into(),
        format!("cli-pairings/{pairing}/confirm"),
    ] {
        let path = format!("/console/api/computers/{id}/{suffix}");
        let response = fixture
            .call(
                Request::post(&path)
                    .header(header::COOKIE, fixture.cookie(false))
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let mut request = fixture
            .mutation(&path, false)
            .body(Body::from("{}"))
            .unwrap();
        request
            .headers_mut()
            .insert(header::ORIGIN, "https://foreign.invalid".parse().unwrap());
        assert_eq!(fixture.call(request).await.status(), StatusCode::FORBIDDEN);
        let response = fixture
            .call(
                fixture
                    .mutation(&path, false)
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await;
        assert_eq!(response.status(), StatusCode::OK); // Synthetic gateway response.
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
    let observed = fixture.observed.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert_eq!(
        observed[0].path,
        format!("/computers/admin/{id}/cli-pairings")
    );
    assert_eq!(
        observed[1].path,
        format!("/computers/admin/{id}/cli-pairings/{pairing}/confirm")
    );
    for request in observed.iter() {
        assert_eq!(
            request.headers[header::ORIGIN],
            fixture.state.config.public_origin()
        );
        assert!(!request.headers.contains_key(header::COOKIE));
    }
}
