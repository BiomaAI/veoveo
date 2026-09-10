//! Two real local relay hops and synthetic leases. The stock client and the
//! worker's restricted gRPC facade require their separate integration fixtures.
use axum::{
    Router,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    http::{HeaderMap, header},
    routing::get,
};
use chrono::{TimeDelta, Utc};
use futures::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Mutex, oneshot};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message as ClientMessage};
use tokio_util::sync::CancellationToken;
use veoveo_computers_contract::*;
use veoveo_computers_transport::{
    CliRelayMode, CliUpstreamRequest, Client, TransportError, relay_cli,
};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
struct Fixture {
    client: Option<Socket>,
    worker: Option<WebSocket>,
    results: Vec<oneshot::Receiver<Result<(), TransportError>>>,
    stop: CancellationToken,
    jobs: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.cancel();
        for job in &self.jobs {
            job.abort();
        }
    }
}
async fn server(router: Router, stop: CancellationToken) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let job = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    (format!("http://{addr}"), job)
}
fn bounded(ws: WebSocketUpgrade) -> WebSocketUpgrade {
    ws.max_frame_size(65536)
        .max_message_size(65536)
        .write_buffer_size(0)
        .max_write_buffer_size(131072)
}
impl Fixture {
    async fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let stop = CancellationToken::new();
        let (sender, worker) = oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(sender)));
        let router = Router::new().route(
            "/cli",
            get(move |headers: HeaderMap, ws: WebSocketUpgrade| {
                let sender = sender.clone();
                async move {
                    assert!(!headers.contains_key(header::ORIGIN));
                    assert!(!headers.contains_key(header::COOKIE));
                    assert_eq!(
                        headers.get(header::AUTHORIZATION).unwrap(),
                        "Bearer synthetic-cli-grant"
                    );
                    bounded(ws).on_upgrade(move |socket| async move {
                        sender
                            .lock()
                            .await
                            .take()
                            .unwrap()
                            .send(socket)
                            .ok()
                            .unwrap();
                    })
                }
            }),
        );
        let (mut url, job) = server(router, stop.clone()).await;
        let mut jobs = vec![job];
        let mut results = Vec::new();
        for mode in [CliRelayMode::Internal, CliRelayMode::PublicClient] {
            let connector = Client::new(reqwest::Client::builder().no_proxy()).unwrap();
            let (sender, result) = oneshot::channel();
            results.push(result);
            let sender = Arc::new(Mutex::new(Some(sender)));
            let token = stop.clone();
            let router = Router::new().route(
                "/cli",
                get(move |ws: WebSocketUpgrade| {
                    let connector = connector.clone();
                    let destination = format!("{url}/cli");
                    let sender = sender.clone();
                    let stop = token.clone();
                    async move {
                        let upstream = connector
                            .connect_cli(CliUpstreamRequest {
                                url: destination.parse().unwrap(),
                                authorization: "Bearer synthetic-cli-grant".parse().unwrap(),
                                host: None,
                            })
                            .await
                            .unwrap();
                        bounded(ws).on_upgrade(move |socket| async move {
                            let result = relay_cli(socket, upstream, mode, stop).await;
                            let _ = sender.lock().await.take().unwrap().send(result);
                        })
                    }
                }),
            );
            let (next, job) = server(router, stop.clone()).await;
            url = next;
            jobs.push(job);
        }
        let (client, _) = tokio::time::timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(format!("{}/cli", url.replace("http://", "ws://"))),
        )
        .await
        .unwrap()
        .unwrap();
        let worker = tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .unwrap()
            .unwrap();
        Self {
            client: Some(client),
            worker: Some(worker),
            results,
            stop,
            jobs,
        }
    }
    async fn send(&mut self, control: TerminalServerControl) {
        self.worker
            .as_mut()
            .unwrap()
            .send(Message::Text(
                serde_json::to_string(&control).unwrap().into(),
            ))
            .await
            .unwrap();
    }
    async fn ready(&mut self) {
        self.send(TerminalServerControl::Ready(TerminalReady {
            version: TERMINAL_VERSION,
            kind: TerminalReadyKind::Ready,
            expires_at: Utc::now() + TimeDelta::seconds(2),
        }))
        .await;
    }
    async fn receive(&mut self) -> ClientMessage {
        tokio::time::timeout(Duration::from_secs(3), self.client.as_mut().unwrap().next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    }
    async fn finishes_with(&mut self, expected: TransportError) {
        let mut results = Vec::new();
        for receiver in self.results.drain(..) {
            results.push(
                tokio::time::timeout(Duration::from_secs(3), receiver)
                    .await
                    .expect("relay missed authority bound")
                    .unwrap(),
            );
        }
        assert!(
            results.contains(&Err(expected)),
            "both relay hops closed: {results:?}"
        );
    }
}

#[tokio::test]
async fn two_hop_cli_preserves_binary_bytes_and_strips_all_internal_controls_across_renewal() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut fixture = Fixture::new().await;
        let preface = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
        fixture
            .client
            .as_mut()
            .unwrap()
            .send(ClientMessage::Binary(preface.to_vec().into()))
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(100),
                fixture.worker.as_mut().unwrap().recv()
            )
            .await
            .is_err()
        );
        fixture.ready().await;
        assert_eq!(
            fixture
                .worker
                .as_mut()
                .unwrap()
                .recv()
                .await
                .unwrap()
                .unwrap(),
            Message::Binary(preface.to_vec().into())
        );
        for sequence in 1..=5 {
            tokio::time::sleep(Duration::from_millis(300)).await;
            fixture
                .send(TerminalServerControl::Lease(TerminalLease {
                    kind: TerminalLeaseKind::Lease,
                    sequence,
                    expires_at: Utc::now() + TimeDelta::seconds(2),
                }))
                .await;
            let bytes = vec![0, 255, 13, 27, sequence as u8];
            fixture
                .worker
                .as_mut()
                .unwrap()
                .send(Message::Binary(bytes.clone().into()))
                .await
                .unwrap();
            assert_eq!(fixture.receive().await, ClientMessage::Binary(bytes.into()));
        }
        fixture.finishes_with(TransportError::Expired).await;
    })
    .await
    .expect("bounded CLI relay scenario");
}

#[tokio::test]
async fn cli_deadlines_close_both_hops_during_blocked_input_or_output() {
    tokio::time::timeout(Duration::from_secs(12), async {
        for blocked_output in [true, false] {
            let mut fixture = Fixture::new().await;
            fixture.ready().await;
            // This actual round trip proves both relays consumed the initial lease.
            fixture
                .worker
                .as_mut()
                .unwrap()
                .send(Message::Binary(vec![1].into()))
                .await
                .unwrap();
            assert_eq!(
                fixture.receive().await,
                ClientMessage::Binary(vec![1].into())
            );
            let job = if blocked_output {
                let mut writer = fixture.worker.take().unwrap();
                tokio::spawn(async move {
                    for _ in 0..1024 {
                        if writer
                            .send(Message::Binary(vec![0; 65536].into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                })
            } else {
                let mut writer = fixture.client.take().unwrap();
                tokio::spawn(async move {
                    for _ in 0..1024 {
                        if writer
                            .send(ClientMessage::Binary(vec![0; 65536].into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                })
            };
            fixture.jobs.push(job);
            fixture.finishes_with(TransportError::Expired).await;
        }
    })
    .await
    .expect("bounded CLI relay scenario");
}

#[tokio::test]
async fn cli_rejects_forged_client_authority_and_invalid_upstream_ordering() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for case in [
            "client_lease",
            "output_before_ready",
            "replay",
            "duplicate_ready",
            "stale_sequence",
        ] {
            let mut fixture = Fixture::new().await;
            match case {
                "output_before_ready" => fixture
                    .worker
                    .as_mut()
                    .unwrap()
                    .send(Message::Binary(vec![1].into()))
                    .await
                    .unwrap(),
                "client_lease" => fixture
                    .client
                    .as_mut()
                    .unwrap()
                    .send(ClientMessage::Text(
                        "{\"kind\":\"lease\",\"sequence\":1}".into(),
                    ))
                    .await
                    .unwrap(),
                _ => {
                    fixture.ready().await;
                    let control = match case {
                        "replay" => TerminalServerControl::ReplayComplete(TerminalReplayComplete {
                            kind: TerminalReplayCompleteKind::ReplayComplete,
                        }),
                        "duplicate_ready" => TerminalServerControl::Ready(TerminalReady {
                            version: TERMINAL_VERSION,
                            kind: TerminalReadyKind::Ready,
                            expires_at: Utc::now() + TimeDelta::seconds(2),
                        }),
                        _ => TerminalServerControl::Lease(TerminalLease {
                            kind: TerminalLeaseKind::Lease,
                            sequence: 0,
                            expires_at: Utc::now() + TimeDelta::seconds(2),
                        }),
                    };
                    fixture.send(control).await;
                }
            }
            fixture.finishes_with(TransportError::Protocol).await;
        }
    })
    .await
    .expect("bounded CLI relay scenario");
}

#[tokio::test]
async fn cli_destination_rejects_url_credentials_query_and_fragment_before_network() {
    let connector = Client::new(reqwest::Client::builder().no_proxy()).unwrap();
    for url in [
        "http://user:secret@127.0.0.1/cli",
        "http://127.0.0.1/cli?token=secret",
        "http://127.0.0.1/cli#secret",
        "file:///tmp/socket",
    ] {
        let result = connector
            .connect_cli(CliUpstreamRequest {
                url: url.parse().unwrap(),
                authorization: "Bearer fixture".parse().unwrap(),
                host: None,
            })
            .await;
        assert!(matches!(result, Err(TransportError::Protocol)));
    }
}
