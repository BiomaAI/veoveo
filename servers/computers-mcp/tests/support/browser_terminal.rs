//! Local native transport evidence. Public ingress and headed Console acceptance are separate.
use crate::{signing::Signing, support};
use chrono::{TimeDelta, Utc};
use futures::{SinkExt, StreamExt};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_computers::{ComputerActor, ComputersStore, api::*, session_grants::SessionGrantPolicy};
use veoveo_computers_mcp::{Application, CapacityHealth, NamedTemplate, RuntimeAccess, Templates};
use veoveo_computers_runtime::{DevelopmentTemplate, OpenShellRuntime};
use veoveo_mcp_contract::{GatewayAction, GatewayInternalIdentity, PolicyRuleId};
use veoveo_task_runtime::TaskRuntime;

type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
pub(crate) struct Server {
    pub(crate) base: String,
    terminal: String,
    pub(crate) origin: String,
    stop: CancellationToken,
    jobs: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.cancel();
        for job in &self.jobs {
            job.abort();
        }
    }
}
impl Server {
    pub(crate) async fn start(
        platform: veoveo_platform_store::PlatformStore,
        runtime: OpenShellRuntime,
        template: DevelopmentTemplate,
        signing: &Signing,
    ) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (health, receiver) = watch::channel(CapacityHealth {
            availability: CapacityAvailability::Available,
            observed_at: Instant::now(),
        });
        let (publisher, access) = RuntimeAccess::channel();
        runtime.ready().await.unwrap();
        publisher.available(runtime.clone());
        let store = ComputersStore::new(platform.clone(), runtime.provider_instance_id()).unwrap();
        let app = Application::new(
            store,
            TaskRuntime::new(platform, "computers", "browser-native"),
            Templates::new(
                vec![NamedTemplate::new("development".into(), template.clone()).unwrap()],
                Some(template.fingerprint()),
            )
            .unwrap(),
            receiver,
            access,
        )
        .unwrap();
        let stop = CancellationToken::new();
        let origin = "https://browser.fixture.invalid".to_owned();
        let router = veoveo_computers_mcp::server::router(
            Arc::new(app),
            signing.verifier.clone(),
            vec![address.to_string()],
            veoveo_computers_mcp::server::BrowserOrigins::new(vec![origin.clone()]).unwrap(),
            stop.clone(),
        )
        .unwrap();
        let token = stop.clone();
        let serve = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(token.cancelled_owned())
                .await
                .unwrap();
        });
        let token = stop.clone();
        let probe = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = token.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(2)) => {},
                }
                if matches!(
                    tokio::time::timeout(Duration::from_secs(5), runtime.ready()).await,
                    Ok(Ok(()))
                ) {
                    publisher.available(runtime.clone());
                    health.send_replace(CapacityHealth {
                        availability: CapacityAvailability::Available,
                        observed_at: Instant::now(),
                    });
                } else {
                    publisher.unavailable();
                }
            }
        });
        let base = format!("http://{address}/computers/admin");
        let (first, first_job) = crate::terminal_hops::start(base.clone(), stop.clone()).await;
        let (terminal, second_job) = crate::terminal_hops::start(first, stop.clone()).await;
        Self {
            base,
            terminal,
            origin,
            stop,
            jobs: vec![serve, probe, first_job, second_job],
        }
    }
    async fn ticket(&self, id: Uuid, bearer: &str) -> TerminalTicket {
        let response = http()
            .post(format!("{}/computers/{id}/terminal-ticket", self.base))
            .bearer_auth(bearer)
            .header("origin", &self.origin)
            .json(&TerminalTicketInput::default())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::CREATED);
        assert_eq!(response.headers()["cache-control"], "no-store");
        response.json().await.unwrap()
    }
    async fn socket(&self, id: Uuid, bearer: &str) -> Socket {
        let mut request = format!(
            "{}/computers/{id}/terminal",
            self.terminal.replace("http://", "ws://")
        )
        .into_client_request()
        .unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {bearer}").parse().unwrap());
        request
            .headers_mut()
            .insert("origin", self.origin.parse().unwrap());
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            tokio_tungstenite::connect_async(request),
        )
        .await
        .unwrap();
        assert!(response.is_ok(), "authenticated terminal upgrade rejected");
        response.ok().unwrap().0
    }
}
fn http() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}
pub(crate) fn bearer(signing: &Signing, identity: &GatewayInternalIdentity) -> String {
    signing.identity(
        identity.clone(),
        "computers",
        Utc::now() + TimeDelta::minutes(2),
    )
}
fn attach(ticket: &TerminalTicket) -> Message {
    Message::Text(
        serde_json::to_string(&TerminalAttach {
            version: TERMINAL_VERSION,
            kind: TerminalAttachKind::Attach,
            computer_id: ticket.computer_id,
            token: TerminalToken::new(ticket.token.expose_secret().into()),
            cols: 90,
            rows: 30,
        })
        .unwrap()
        .into(),
    )
}
async fn replay(socket: &mut Socket) {
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut ready = false;
        let mut bytes = 0;
        loop {
            match socket
                .next()
                .await
                .expect("terminal ended before replay")
                .expect("terminal replay transport failed")
            {
                Message::Text(text) => {
                    match serde_json::from_str::<TerminalServerControl>(&text).unwrap() {
                        TerminalServerControl::Ready(value) => {
                            assert_eq!(value.version, TERMINAL_VERSION);
                            assert!(!ready);
                            ready = true;
                        }
                        TerminalServerControl::ReplayComplete(_) => {
                            assert!(ready);
                            return;
                        }
                        TerminalServerControl::Lease(value) => {
                            assert!(ready && value.sequence > 0);
                        }
                    }
                }
                Message::Binary(data) => {
                    assert!(ready);
                    bytes += data.len();
                    assert!(bytes <= 256 * 1024);
                }
                _ => panic!("unexpected replay frame"),
            }
        }
    })
    .await
    .expect("native replay deadline");
}
async fn command(socket: &mut Socket, suffix: &str) {
    // The complete expected output is absent from the input, so terminal echo cannot pass this check.
    let command = format!("printf 'browser-%s\\n' '{suffix}'\r");
    socket
        .send(Message::Binary(command.into_bytes().into()))
        .await
        .unwrap();
    let expected = format!("browser-{suffix}");
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut output = Vec::new();
        loop {
            if let Message::Binary(data) = socket
                .next()
                .await
                .expect("terminal ended during command")
                .expect("terminal command transport failed")
            {
                output.extend_from_slice(&data);
                assert!(output.len() <= 256 * 1024, "bounded command output");
                if output
                    .windows(expected.len())
                    .any(|w| w == expected.as_bytes())
                {
                    return;
                }
            }
        }
    })
    .await
    .expect("native command output deadline");
}
async fn closed(socket: &mut Socket) {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut bytes = 0;
        loop {
            match socket.next().await {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return,
                Some(Ok(message)) => {
                    bytes += message.len();
                    assert!(bytes <= 256 * 1024);
                }
            }
        }
    })
    .await
    .expect("revocation did not close native access within five seconds");
}
pub async fn qualify(
    db: &support::TestDb,
    runtime: OpenShellRuntime,
    template: DevelopmentTemplate,
    computer: Uuid,
) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut control = support::policy::control();
    let mut read = control.policies[0].rules[0].clone();
    read.id = PolicyRuleId::new("computer-browser").unwrap();
    read.actions = [GatewayAction::ResourcesRead, GatewayAction::ComputerAttach]
        .into_iter()
        .collect();
    read.tools.clear();
    control.policies[0].rules.push(read);
    support::policy::install(&db.a, control.clone()).await;
    let store = ComputersStore::new(db.a.clone(), runtime.provider_instance_id()).unwrap();
    store
        .install_session_grant_policy(
            None,
            SessionGrantPolicy {
                max_grants: 4,
                absolute_seconds: 120,
                idle_seconds: 60,
            },
        )
        .await
        .unwrap();
    let signing = Signing::new();
    let alice = support::browser::identity(db, "alice").await;
    let bob = support::browser::identity(db, "bob").await;
    let a = Server::start(db.a.clone(), runtime.clone(), template.clone(), &signing).await;
    let b = Server::start(db.b.clone(), runtime, template, &signing).await;
    let auth = bearer(&signing, &alice);
    let view: ComputerView = http()
        .get(format!("{}/computers/{computer}", a.base))
        .bearer_auth(&auth)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(view.can_connect && view.can_stop && !view.busy);
    for origin in [None, Some("null"), Some("https://untrusted.invalid")] {
        let mut request = http()
            .post(format!("{}/computers/{computer}/terminal-ticket", a.base))
            .bearer_auth(&auth)
            .json(&TerminalTicketInput::default());
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        assert_eq!(
            request.send().await.unwrap().status(),
            reqwest::StatusCode::FORBIDDEN
        );
    }
    assert_eq!(
        http()
            .post(format!("{}/computers/{computer}/terminal-ticket", a.base))
            .bearer_auth(bearer(&signing, &bob))
            .header("origin", &a.origin)
            .json(&TerminalTicketInput::default())
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::NOT_FOUND
    );
    let ticket = a.ticket(computer, &auth).await;
    let mut socket = b.socket(computer, &auth).await;
    socket.send(attach(&ticket)).await.unwrap();
    replay(&mut socket).await;
    command(&mut socket, "first").await;
    let mut replayed = a.socket(computer, &auth).await;
    replayed.send(attach(&ticket)).await.unwrap();
    closed(&mut replayed).await;
    socket
        .send(Message::Text(
            serde_json::to_string(&TerminalResize {
                kind: TerminalResizeKind::Resize,
                cols: 100,
                rows: 35,
            })
            .unwrap()
            .into(),
        ))
        .await
        .unwrap();
    command(&mut socket, "after-resize").await;
    let mut denied = control.clone();
    denied.policies[0]
        .rules
        .last_mut()
        .unwrap()
        .actions
        .remove(&GatewayAction::ComputerAttach);
    support::policy::install(&db.b, denied).await;
    closed(&mut socket).await;
    support::policy::install(&db.b, control).await;
    let mut malformed = a.socket(computer, &auth).await;
    malformed
        .send(Message::Text("{\"version\":1,\"type\":\"attach\"}".into()))
        .await
        .unwrap();
    closed(&mut malformed).await;

    // A new access token is valid for admission, then expires while the retained grant renews.
    let mut short = alice.clone();
    let end = Utc::now() + TimeDelta::seconds(8);
    short
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .expires_at = end;
    short.expires_at = end;
    let auth = bearer(&signing, &short);
    let ticket = b.ticket(computer, &auth).await;
    let mut socket = a.socket(computer, &auth).await;
    socket.send(attach(&ticket)).await.unwrap();
    replay(&mut socket).await;
    // Cross source-token expiry and the original service lease through both
    // relay hops without reconnecting the native shell.
    tokio::time::sleep(Duration::from_secs(31)).await;
    command(&mut socket, "after-token-expiry").await;
    let family = veoveo_platform_store::gateway_refresh_family_record_id(
        Uuid::parse_str(
            alice
                .request_context
                .as_ref()
                .unwrap()
                .access_token
                .session_family
                .as_ref()
                .unwrap()
                .as_str(),
        )
        .unwrap(),
    );
    db.b.client()
        .query("UPDATE ONLY $family SET revoked_at = time::now();")
        .bind(("family", family))
        .await
        .unwrap()
        .check()
        .unwrap();
    closed(&mut socket).await;
    let current = store
        .get(
            ComputerActor::from_verified(&alice).unwrap().owner(),
            computer,
        )
        .await
        .unwrap();
    assert_eq!(
        current.phase,
        ComputerPhase::Ready,
        "revocation must preserve native execution"
    );
    assert!(current.active_operation.is_none());
    support::policy::install_default(&db.a).await;
}
