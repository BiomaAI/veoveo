//! Real local WebSockets and synthetic authority. No domain or provider qualification.
use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    routing::get,
};
use chrono::{TimeDelta, Utc};
use futures::{SinkExt, StreamExt};
use std::{sync::Arc, time::Duration};
use tokio::sync::{Mutex, oneshot};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message as ClientMessage};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_computers_contract::*;
use veoveo_computers_transport::{Client, TransportError, UpstreamRequest, relay};

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
struct Fixture {
    client: Option<Socket>,
    provider: Option<WebSocket>,
    finished: Option<oneshot::Receiver<Result<(), TransportError>>>,
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
    let address = listener.local_addr().unwrap();
    let job = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    (format!("http://{address}"), job)
}
impl Fixture {
    async fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let stop = CancellationToken::new();
        let computer = Uuid::now_v7();
        let (send_provider, receive_provider) = oneshot::channel();
        let provider = Router::new()
            .route(
                "/terminal",
                get(
                    |State(sender): State<Arc<Mutex<Option<oneshot::Sender<WebSocket>>>>>,
                     ws: WebSocketUpgrade| async move {
                        ws.max_frame_size(65536).max_message_size(65536).on_upgrade(
                            move |socket| async move {
                                sender
                                    .lock()
                                    .await
                                    .take()
                                    .unwrap()
                                    .send(socket)
                                    .ok()
                                    .unwrap();
                            },
                        )
                    },
                ),
            )
            .with_state(Arc::new(Mutex::new(Some(send_provider))));
        let (url, origin_job) = server(provider, stop.clone()).await;
        let connector = Client::new(reqwest::Client::builder().no_proxy()).unwrap();
        let (send_finished, finished) = oneshot::channel();
        let send_finished = Arc::new(Mutex::new(Some(send_finished)));
        let token = stop.clone();
        let relay_server = Router::new().route(
            "/terminal",
            get(move |ws: WebSocketUpgrade| {
                let connector = connector.clone();
                let url = url.clone();
                let finished = send_finished.clone();
                let stop = token.clone();
                async move {
                    let upstream = connector
                        .connect(UpstreamRequest {
                            url: format!("{url}/terminal").parse().unwrap(),
                            authorization: "Bearer fixture".parse().unwrap(),
                            host: None,
                            origin: "https://fixture.invalid".parse().unwrap(),
                        })
                        .await
                        .unwrap();
                    ws.max_frame_size(65536)
                        .max_message_size(65536)
                        .write_buffer_size(0)
                        .max_write_buffer_size(131072)
                        .on_upgrade(move |socket| async move {
                            let result = relay(socket, upstream, computer, stop).await;
                            let _ = finished.lock().await.take().unwrap().send(result);
                        })
                }
            }),
        );
        let (url, relay_job) = server(relay_server, stop.clone()).await;
        let (mut client, _) = tokio::time::timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(format!(
                "{}/terminal",
                url.replace("http://", "ws://")
            )),
        )
        .await
        .unwrap()
        .unwrap();
        client
            .send(ClientMessage::Text(
                serde_json::to_string(&TerminalAttach {
                    version: TERMINAL_VERSION,
                    kind: TerminalAttachKind::Attach,
                    computer_id: computer,
                    token: TerminalToken::new("fixture-ticket".into()),
                    cols: 80,
                    rows: 24,
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();
        let mut provider = tokio::time::timeout(Duration::from_secs(5), receive_provider)
            .await
            .unwrap()
            .unwrap();
        let Some(Ok(Message::Text(text))) =
            tokio::time::timeout(Duration::from_secs(5), provider.recv())
                .await
                .unwrap()
        else {
            panic!("missing bounded first frame");
        };
        assert_eq!(
            serde_json::from_str::<TerminalAttach>(&text)
                .unwrap()
                .computer_id,
            computer
        );
        Self {
            client: Some(client),
            provider: Some(provider),
            finished: Some(finished),
            stop,
            jobs: vec![origin_job, relay_job],
        }
    }
    async fn send(&mut self, control: TerminalServerControl) {
        self.provider
            .as_mut()
            .unwrap()
            .send(Message::Text(
                serde_json::to_string(&control).unwrap().into(),
            ))
            .await
            .unwrap();
    }
    async fn begin(&mut self) {
        self.send(TerminalServerControl::Ready(TerminalReady {
            version: TERMINAL_VERSION,
            kind: TerminalReadyKind::Ready,
            expires_at: Utc::now() + TimeDelta::seconds(2),
        }))
        .await;
        self.control().await;
        self.send(TerminalServerControl::ReplayComplete(
            TerminalReplayComplete {
                kind: TerminalReplayCompleteKind::ReplayComplete,
            },
        ))
        .await;
        self.control().await;
    }
    async fn control(&mut self) -> TerminalServerControl {
        let message =
            tokio::time::timeout(Duration::from_secs(3), self.client.as_mut().unwrap().next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        let ClientMessage::Text(text) = message else {
            panic!("expected terminal control")
        };
        serde_json::from_str(&text).unwrap()
    }
    async fn result(&mut self) -> Result<(), TransportError> {
        tokio::time::timeout(Duration::from_secs(3), self.finished.take().unwrap())
            .await
            .expect("relay missed the authority bound")
            .unwrap()
    }
}
#[tokio::test]
async fn sequenced_upstream_renewal_preserves_bytes_beyond_the_initial_deadline() {
    tokio::time::timeout(Duration::from_secs(10), async {
    let mut fixture = Fixture::new().await;
    fixture.begin().await;
    for sequence in 1..=5 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        fixture
            .send(TerminalServerControl::Lease(TerminalLease {
                kind: TerminalLeaseKind::Lease,
                sequence,
                expires_at: Utc::now() + TimeDelta::seconds(2),
            }))
            .await;
        assert!(
            matches!(fixture.control().await, TerminalServerControl::Lease(value) if value.sequence == sequence)
        );
    }
    let bytes = vec![0, 255, 27, 13];
    fixture
        .client
        .as_mut()
        .unwrap()
        .send(ClientMessage::Binary(bytes.clone().into()))
        .await
        .unwrap();
    assert_eq!(
        fixture
            .provider
            .as_mut()
            .unwrap()
            .recv()
            .await
            .unwrap()
            .unwrap(),
        Message::Binary(bytes.clone().into())
    );
    fixture
        .provider
        .as_mut()
        .unwrap()
        .send(Message::Binary(bytes.clone().into()))
        .await
        .unwrap();
    assert_eq!(
        fixture
            .client
            .as_mut()
            .unwrap()
            .next()
            .await
            .unwrap()
            .unwrap(),
        ClientMessage::Binary(bytes.into())
    );
    assert_eq!(fixture.result().await, Err(TransportError::Expired));
    }).await.expect("bounded relay scenario");
}
#[tokio::test]
async fn expired_authority_closes_independently_of_both_backpressured_directions() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for blocked_output in [true, false] {
            let mut fixture = Fixture::new().await;
            fixture.begin().await;
            let job = if blocked_output {
                let mut producer = fixture.provider.take().unwrap();
                tokio::spawn(async move {
                    for _ in 0..1024 {
                        if producer
                            .send(Message::Binary(vec![0; 65536].into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                })
            } else {
                let mut producer = fixture.client.take().unwrap();
                tokio::spawn(async move {
                    for _ in 0..1024 {
                        if producer
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
            assert_eq!(fixture.result().await, Err(TransportError::Expired));
        }
    })
    .await
    .expect("bounded relay scenario");
}
#[tokio::test]
async fn input_before_replay_and_forged_client_lease_controls_fail_closed() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for binary in [true, false] {
            let mut fixture = Fixture::new().await;
            if !binary {
                fixture.begin().await;
            }
            let message = if binary {
                ClientMessage::Binary(vec![1].into())
            } else {
                ClientMessage::Text(
                    serde_json::to_string(&TerminalServerControl::Lease(TerminalLease {
                        kind: TerminalLeaseKind::Lease,
                        sequence: 1,
                        expires_at: Utc::now() + TimeDelta::seconds(30),
                    }))
                    .unwrap()
                    .into(),
                )
            };
            fixture
                .client
                .as_mut()
                .unwrap()
                .send(message)
                .await
                .unwrap();
            assert_eq!(fixture.result().await, Err(TransportError::Protocol));
        }
    })
    .await
    .expect("bounded relay scenario");
}
