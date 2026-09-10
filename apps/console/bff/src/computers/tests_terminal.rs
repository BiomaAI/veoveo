use super::*;
use axum::extract::{
    Path,
    ws::{Message, WebSocketUpgrade},
};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as ClientMessage, client::IntoClientRequest},
};
use veoveo_computers_contract as api_contract;

pub(super) fn upstream(capture: Arc<Mutex<Vec<Observed>>>) -> Router {
    Router::new().route(
        "/computers/admin/{id}/terminal",
        get(
            move |Path(id): Path<Uuid>, headers: HeaderMap, ws: WebSocketUpgrade| {
                capture.lock().unwrap().push(Observed {
                    path: format!("/computers/admin/{id}/terminal"),
                    headers,
                });
                async move {
                    ws.on_upgrade(move |mut socket| async move {
                        let Some(Ok(Message::Text(first))) = socket.recv().await else {
                            return;
                        };
                        let first: api_contract::TerminalAttach =
                            serde_json::from_str(&first).unwrap();
                        assert_eq!(first.computer_id, id);
                        assert_eq!(first.token.expose_secret(), "fixture-first-frame");
                        let ready = api_contract::TerminalReady {
                            version: api_contract::TERMINAL_VERSION,
                            kind: api_contract::TerminalReadyKind::Ready,
                            expires_at: Utc::now() + chrono::TimeDelta::seconds(20),
                        };
                        socket
                            .send(Message::Text(serde_json::to_string(&ready).unwrap().into()))
                            .await
                            .unwrap();
                        socket
                            .send(Message::Text("{\"type\":\"replay_complete\"}".into()))
                            .await
                            .unwrap();
                        while let Some(Ok(message)) = socket.recv().await {
                            if let Message::Binary(bytes) = message
                                && socket.send(Message::Binary(bytes)).await.is_err()
                            {
                                break;
                            }
                        }
                    })
                }
            },
        ),
    )
}

pub(super) async fn listen(fixture: &mut Fixture) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("ws://{}", listener.local_addr().unwrap());
    let router = fixture.router.clone();
    fixture.jobs.push(tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    }));
    base
}

#[tokio::test]
async fn cookie_authenticated_upgrade_returns_refresh_cookie_and_relays_terminal_bytes() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut fixture = Fixture::new().await;
        let base = listen(&mut fixture).await;
        let id = Uuid::new_v4();
        let mut request = format!("{base}/console/api/computers/{id}/terminal")
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            header::COOKIE,
            HeaderValue::from_str(&fixture.cookie(true)).unwrap(),
        );
        request.headers_mut().insert(
            header::ORIGIN,
            HeaderValue::from_str(&fixture.state.config.public_origin()).unwrap(),
        );
        let (mut socket, response) = connect_async(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
        assert!(response.headers().contains_key(header::SET_COOKIE));
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let attach = api_contract::TerminalAttach {
            version: api_contract::TERMINAL_VERSION,
            kind: api_contract::TerminalAttachKind::Attach,
            computer_id: id,
            token: api_contract::TerminalToken::new("fixture-first-frame".into()),
            cols: 80,
            rows: 24,
        };
        socket
            .send(ClientMessage::Text(
                serde_json::to_string(&attach).unwrap().into(),
            ))
            .await
            .unwrap();
        let ready = socket.next().await.unwrap().unwrap();
        assert!(matches!(
            serde_json::from_str::<api_contract::TerminalServerControl>(ready.to_text().unwrap())
                .unwrap(),
            api_contract::TerminalServerControl::Ready(_)
        ));
        let replay = socket.next().await.unwrap().unwrap();
        assert_eq!(replay.to_text().unwrap(), "{\"type\":\"replay_complete\"}");
        socket
            .send(ClientMessage::Binary(b"bounded input".to_vec().into()))
            .await
            .unwrap();
        assert_eq!(
            socket.next().await.unwrap().unwrap().into_data(),
            b"bounded input".as_slice()
        );
        {
            let observed = fixture.observed.lock().unwrap();
            assert_eq!(observed.len(), 2);
            assert_eq!(
                observed[1].headers[header::AUTHORIZATION],
                "Bearer rotated-fixture-access"
            );
            assert_eq!(
                observed[1].headers[header::ORIGIN],
                fixture.state.config.public_origin()
            );
            assert!(!observed[1].headers.contains_key(header::COOKIE));
        }
        fixture.state.computers.stop.cancel();
        assert!(!matches!(
            tokio::time::timeout(Duration::from_secs(2), socket.next())
                .await
                .unwrap(),
            Some(Ok(ClientMessage::Binary(_) | ClientMessage::Text(_)))
        ));
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn upgrade_rejects_missing_cookie_duplicate_origin_and_url_credentials_before_gateway() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut fixture = Fixture::new().await;
        let base = listen(&mut fixture).await;
        let id = Uuid::new_v4();
        for (cookie, duplicate, query, expected) in [
            (false, false, "", StatusCode::UNAUTHORIZED),
            (true, true, "", StatusCode::FORBIDDEN),
            (true, false, "?token=forged", StatusCode::BAD_REQUEST),
        ] {
            let mut request = format!("{base}/console/api/computers/{id}/terminal{query}")
                .into_client_request()
                .unwrap();
            let origin = HeaderValue::from_str(&fixture.state.config.public_origin()).unwrap();
            request.headers_mut().insert(header::ORIGIN, origin.clone());
            if duplicate {
                request.headers_mut().append(header::ORIGIN, origin);
            }
            if cookie {
                request.headers_mut().insert(
                    header::COOKIE,
                    HeaderValue::from_str(&fixture.cookie(true)).unwrap(),
                );
            }
            match connect_async(request).await {
                Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                    assert_eq!(response.status(), expected)
                }
                _ => panic!("expected rejected upgrade"),
            }
        }
        assert!(fixture.observed.lock().unwrap().is_empty());
    })
    .await
    .unwrap();
}
