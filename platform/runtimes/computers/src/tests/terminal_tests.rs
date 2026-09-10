use super::*;
use crate::{TerminalOutput, protocol::terminal::v1 as terminal_api};
use prost::Message;
use russh::{Channel, ChannelId, Pty, server};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[path = "renewal_tests.rs"]
mod renewal_tests;
#[path = "replay_tests.rs"]
mod replay_tests;
#[derive(Clone, Default)]
pub(super) struct Gate {
    reached: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}
impl Gate {
    pub(super) async fn pause(&self) {
        self.reached.notify_one();
        self.release.notified().await;
    }
    async fn wait(&self) {
        tokio::time::timeout(Duration::from_secs(3), self.reached.notified())
            .await
            .unwrap();
    }
}
#[derive(Default)]
pub(super) struct SshState {
    pub size: (u32, u32),
    pub subsystems: Vec<String>,
    pub bytes: Vec<u8>,
    pub eofs: usize,
    pub pty_gate: Option<Gate>,
    pub initial_output: Option<Vec<Vec<u8>>>,
    pub initial_output_eof: bool,
    pub events_requested: bool,
    pub omit_replay: bool,
    pub replay_metadata: Option<Vec<u8>>,
    pub duplicate_replay: bool,
    pub live_output: Vec<Vec<u8>>,
    pub data_gate: Option<Gate>,
}
pub(super) fn key() -> russh::keys::PrivateKey {
    // Deterministic fixture-only key, never an installation credential.
    russh::keys::PrivateKey::new(
        russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[7; 32]).into(),
        "local generated gRPC fixture",
    )
    .unwrap()
}
struct Ssh(Fake);
impl server::Handler for Ssh {
    type Error = russh::Error;
    async fn auth_none(&mut self, user: &str) -> std::result::Result<server::Auth, Self::Error> {
        assert_eq!(user, "sandbox");
        Ok(server::Auth::Accept)
    }
    async fn channel_open_session(
        &mut self,
        _: Channel<server::Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut server::Session,
    ) -> std::result::Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        cols: u32,
        rows: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        session: &mut server::Session,
    ) -> std::result::Result<(), Self::Error> {
        assert_eq!(term, "xterm-256color");
        self.0.0.lock().unwrap().ssh.size = (cols, rows);
        let gate = self.0.0.lock().unwrap().ssh.pty_gate.clone();
        if let Some(gate) = gate {
            gate.pause().await;
        }
        session.channel_success(channel)?;
        Ok(())
    }
    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut server::Session,
    ) -> std::result::Result<(), Self::Error> {
        assert_eq!(name, "openshell-main");
        assert!(self.0.0.lock().unwrap().ssh.events_requested);
        self.0.0.lock().unwrap().ssh.subsystems.push(name.into());
        session.channel_success(channel)?;
        let output = self
            .0
            .0
            .lock()
            .unwrap()
            .ssh
            .initial_output
            .clone()
            .unwrap_or_else(|| vec![vec![0, 255, 13, 10]]);
        for frame in output {
            session.data(channel, frame)?;
        }
        {
            let state = self.0.0.lock().unwrap();
            if !state.ssh.omit_replay {
                let metadata = state.ssh.replay_metadata.clone().unwrap_or_else(|| {
                    terminal_api::MainProcessEvent {
                        event: Some(terminal_api::main_process_event::Event::ReplayComplete(
                            terminal_api::MainProcessReplayComplete { next_sequence: 42 },
                        )),
                    }
                    .encode_to_vec()
                });
                session.extended_data(
                    channel,
                    crate::terminal_output::MAIN_EVENT_EXTENDED_DATA,
                    metadata.clone(),
                )?;
                if state.ssh.duplicate_replay {
                    session.extended_data(
                        channel,
                        crate::terminal_output::MAIN_EVENT_EXTENDED_DATA,
                        metadata,
                    )?;
                }
            }
            for frame in &state.ssh.live_output {
                session.data(channel, frame.clone())?;
            }
        }
        if self.0.0.lock().unwrap().ssh.initial_output_eof {
            session.eof(channel)?;
        }
        Ok(())
    }
    async fn env_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        value: &str,
        session: &mut server::Session,
    ) -> std::result::Result<(), Self::Error> {
        assert_eq!((name, value), ("OPENSHELL_MAIN_EVENTS", "1"));
        self.0.0.lock().unwrap().ssh.events_requested = true;
        session.channel_success(channel)?;
        Ok(())
    }
    async fn window_change_request(
        &mut self,
        _: ChannelId,
        cols: u32,
        rows: u32,
        _: u32,
        _: u32,
        _: &mut server::Session,
    ) -> std::result::Result<(), Self::Error> {
        self.0.0.lock().unwrap().ssh.size = (cols, rows);
        Ok(())
    }
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut server::Session,
    ) -> std::result::Result<(), Self::Error> {
        let gate = self.0.0.lock().unwrap().ssh.data_gate.clone();
        if let Some(gate) = gate {
            gate.pause().await;
        }
        self.0.0.lock().unwrap().ssh.bytes.extend(data);
        session.data(channel, data.to_vec())?;
        Ok(())
    }
    async fn channel_eof(
        &mut self,
        _: ChannelId,
        _: &mut server::Session,
    ) -> std::result::Result<(), Self::Error> {
        self.0.0.lock().unwrap().ssh.eofs += 1;
        Ok(())
    }
}
pub(super) async fn forward(
    fake: Fake,
    request: Request<tonic::Streaming<api::TcpForwardFrame>>,
) -> std::result::Result<Response<BoxStream<api::TcpForwardFrame>>, Status> {
    fake.authorize()?;
    let mut input = request.into_inner();
    let Some(api::tcp_forward_frame::Payload::Init(init)) =
        input.message().await?.and_then(|f| f.payload)
    else {
        return Err(Status::invalid_argument("init required"));
    };
    assert_eq!(init.sandbox_id, "sandbox-1");
    assert_eq!(init.service_id, "ssh-proxy:sandbox-1");
    assert_eq!(init.authorization_token, "local-fixture-token");
    assert!(matches!(
        init.target,
        Some(api::tcp_forward_init::Target::Ssh(_))
    ));
    let probe = fake.0.lock().unwrap().forward_probe.clone();
    if let Some(probe) = probe {
        return Ok(super::forward_tests::response(probe, input));
    }
    let (remote, relay) = tokio::io::duplex(MAX_CHUNK_BYTES * 2);
    let (mut reader, mut writer) = tokio::io::split(relay);
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let config = server::Config {
        keys: vec![key()],
        auth_rejection_time: Duration::ZERO,
        ..Default::default()
    };
    tokio::spawn(async move {
        let ssh = async {
            if let Ok(session) = server::run_stream(Arc::new(config), remote, Ssh(fake)).await {
                let _ = session.await;
            }
        };
        let incoming = async {
            while let Ok(Some(frame)) = input.message().await {
                let bytes = crate::terminal::forward_data(frame).unwrap();
                if writer.write_all(&bytes).await.is_err() {
                    break;
                }
            }
        };
        let outgoing = async {
            let mut bytes = vec![0; MAX_CHUNK_BYTES];
            while let Ok(size) = reader.read(&mut bytes).await {
                if size == 0 {
                    break;
                }
                if tx
                    .send(Ok(api::TcpForwardFrame {
                        payload: Some(api::tcp_forward_frame::Payload::Data(
                            bytes[..size].to_vec(),
                        )),
                    }))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        };
        tokio::select! {_=ssh=>{},_=incoming=>{},_=outgoing=>{}}
    });
    Ok(Response::new(Box::pin(stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|v| (v, rx)) },
    ))))
}
#[tokio::test]
async fn generated_grpc_tunnel_uses_retained_ssh_bytes_resize_and_revoke() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    let mut terminal = running
        .runtime
        .attach(
            &binding(),
            TerminalSize::new(100, 30).unwrap(),
            running.lease(Duration::from_secs(30)),
        )
        .await
        .unwrap();
    assert_eq!(terminal.main_process_instance_id(), "main-1");
    assert_eq!(terminal.sandbox_id(), "sandbox-1");
    assert!(terminal.read().await.unwrap().unwrap() == TerminalOutput::Data(vec![0, 255, 13, 10]));
    assert!(terminal.read().await.unwrap().unwrap() == TerminalOutput::ReplayComplete);
    terminal
        .resize(TerminalSize::new(80, 24).unwrap())
        .await
        .unwrap();
    terminal.write(&[255, 0, 27]).await.unwrap();
    assert!(terminal.read().await.unwrap().unwrap() == TerminalOutput::Data(vec![255, 0, 27]));
    terminal.detach().await.unwrap();
    let state = running.fake.0.lock().unwrap();
    assert_eq!(state.ssh.size, (80, 24));
    assert_eq!(state.ssh.subsystems, ["openshell-main"]);
    assert_eq!(state.ssh.eofs, 0);
    assert_eq!(state.revokes, 1);
    assert_eq!(state.stops, 0);
}
#[tokio::test]
async fn terminal_auth_mismatch_host_key_and_expiry_revoke() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    for mode in 1..=3 {
        running.fake.0.lock().unwrap().session_mode = mode;
        assert!(
            running
                .runtime
                .attach(
                    &binding(),
                    TerminalSize::new(100, 30).unwrap(),
                    running.lease(Duration::from_secs(30))
                )
                .await
                .is_err()
        );
    }
    // Failure is delivered before cleanup; wait for the owned cleanup work.
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if running.fake.0.lock().unwrap().revokes == 3 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(running.fake.0.lock().unwrap().stops, 0);
}
#[tokio::test]
async fn idle_terminal_expiry_and_handle_drop_revoke_without_process_stop() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    let terminal = running
        .runtime
        .attach(
            &binding(),
            TerminalSize::new(100, 30).unwrap(),
            running.lease(Duration::from_millis(1200)),
        )
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if running.fake.0.lock().unwrap().revokes == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(terminal);
    let terminal = running
        .runtime
        .attach(
            &binding(),
            TerminalSize::new(100, 30).unwrap(),
            running.lease(Duration::from_secs(30)),
        )
        .await
        .unwrap();
    drop(terminal);
    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if running.fake.0.lock().unwrap().revokes == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(running.fake.0.lock().unwrap().stops, 0);
}
#[tokio::test]
async fn failed_revoke_is_reported_by_explicit_detach() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    let terminal = running
        .runtime
        .attach(
            &binding(),
            TerminalSize::new(100, 30).unwrap(),
            running.lease(Duration::from_secs(30)),
        )
        .await
        .unwrap();
    running.fake.0.lock().unwrap().session_mode = 4;
    assert!(matches!(
        terminal.detach().await,
        Err(RuntimeFailure::TerminalFailed)
    ));
    assert_eq!(running.fake.0.lock().unwrap().revokes, 1);
    assert_eq!(running.fake.0.lock().unwrap().stops, 0);
}

async fn wait_for_revoke(fake: &Fake) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if fake.0.lock().unwrap().revokes == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let state = fake.0.lock().unwrap();
    assert_eq!(state.revokes, 1);
    assert_eq!(state.stops, 0);
    assert_eq!(state.ssh.eofs, 0);
}

#[tokio::test]
async fn cancelled_attach_drains_and_revokes_late_mint_response() {
    let running = Running::start().await;
    let gate = Gate::default();
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.session_reply_gate = Some(gate.clone());
    }
    let runtime = running.runtime.clone();
    let lease = running.lease(Duration::from_secs(30));
    let task = tokio::spawn(async move {
        runtime
            .attach(&binding(), TerminalSize::new(80, 24).unwrap(), lease)
            .await
    });
    gate.wait().await;
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    assert_eq!(running.fake.0.lock().unwrap().revokes, 0);
    gate.release.notify_one();
    wait_for_revoke(&running.fake).await;
    assert!(running.fake.0.lock().unwrap().ssh.subsystems.is_empty());
}

#[tokio::test]
async fn cancelled_attach_after_token_receipt_revokes_during_pty_setup() {
    let running = Running::start().await;
    let gate = Gate::default();
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.pty_gate = Some(gate.clone());
    }
    let runtime = running.runtime.clone();
    let lease = running.lease(Duration::from_secs(30));
    let task = tokio::spawn(async move {
        runtime
            .attach(&binding(), TerminalSize::new(80, 24).unwrap(), lease)
            .await
    });
    gate.wait().await;
    task.abort();
    assert!(matches!(task.await, Err(error) if error.is_cancelled()));
    wait_for_revoke(&running.fake).await;
    gate.release.notify_one();
    assert!(running.fake.0.lock().unwrap().ssh.subsystems.is_empty());
}

#[tokio::test]
async fn uncollected_terminal_handoff_revokes_without_process_stop() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    let runtime = running.runtime.clone();
    let lease = running.lease(Duration::from_secs(30));
    let (ready, received) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let terminal = runtime
            .attach(&binding(), TerminalSize::new(80, 24).unwrap(), lease)
            .await
            .unwrap();
        ready.send(()).unwrap();
        terminal
    });
    tokio::time::timeout(Duration::from_secs(3), received)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // No caller collects the successful Terminal result. Its Drop still owns
    // cancellation even in this ready-result handoff window.
    drop(task);
    wait_for_revoke(&running.fake).await;
}

fn banner_fixture(running: &Running, frames: u8) -> Vec<Vec<u8>> {
    let output: Vec<Vec<u8>> = (0..frames)
        .map(|index| [0, 255, index, 10].repeat(MAX_CHUNK_BYTES / 4))
        .collect();
    let mut state = running.fake.0.lock().unwrap();
    state.sandbox = Some(sandbox(Phase::Ready));
    state.ssh.initial_output = Some(output.clone());
    output
}

#[tokio::test]
async fn retained_replay_batches_small_frames_without_changing_any_bytes() {
    let running = Running::start().await;
    let frames: Vec<Vec<u8>> = (0_u32..16_384)
        .map(|index| {
            let mut frame = index.to_be_bytes().to_vec();
            frame.extend_from_slice(&[0, 255, 27, 13, 10, 7, 8, 9].repeat(4));
            frame
        })
        .collect();
    let expected = frames.concat();
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.initial_output = Some(frames);
    }
    let mut terminal = running
        .runtime
        .attach(
            &binding(),
            TerminalSize::new(100, 30).unwrap(),
            running.lease(Duration::from_secs(30)),
        )
        .await
        .unwrap();
    let started = tokio::time::Instant::now();
    let (actual, chunks) = tokio::time::timeout(Duration::from_secs(10), async {
        let mut actual = Vec::new();
        let mut chunks = 0;
        while actual.len() < expected.len() {
            let TerminalOutput::Data(bytes) = terminal.read().await.unwrap().unwrap() else {
                panic!("early replay boundary")
            };
            assert!(!bytes.is_empty() && bytes.len() <= MAX_CHUNK_BYTES);
            actual.extend_from_slice(&bytes);
            chunks += 1;
        }
        (actual, chunks)
    })
    .await
    .unwrap();
    assert_eq!(actual, expected);
    eprintln!(
        "retained replay fixture: {} bytes, {chunks} output chunks, {:?}",
        actual.len(),
        started.elapsed()
    );
    terminal.detach().await.unwrap();
    wait_for_revoke(&running.fake).await;
    assert!(
        chunks < 1_024,
        "small historical frames were not batched: {chunks}"
    );
    let state = running.fake.0.lock().unwrap();
    assert_eq!(state.ssh.subsystems, ["openshell-main"]);
    assert_eq!(state.ssh.eofs, 0);
}

#[tokio::test]
async fn final_partial_output_batch_precedes_eof_and_revocation() {
    let running = Running::start().await;
    let expected = [0, 255, 27, 13, 10, 7, 8, 9];
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.initial_output = Some(expected.chunks(2).map(<[u8]>::to_vec).collect());
        state.ssh.initial_output_eof = true;
    }
    let mut terminal = running
        .runtime
        .attach(
            &binding(),
            TerminalSize::new(100, 30).unwrap(),
            running.lease(Duration::from_secs(30)),
        )
        .await
        .unwrap();
    let actual = tokio::time::timeout(Duration::from_secs(3), async {
        let mut actual = Vec::new();
        while let Some(bytes) = terminal.read().await.unwrap() {
            if let TerminalOutput::Data(bytes) = bytes {
                actual.extend(bytes);
            }
        }
        actual
    })
    .await
    .unwrap();
    assert_eq!(actual, expected);
    terminal.detach().await.unwrap();
    wait_for_revoke(&running.fake).await;
    assert_eq!(running.fake.0.lock().unwrap().ssh.eofs, 0);
}

#[tokio::test]
async fn queued_banner_does_not_block_write_or_resize_before_reads() {
    let running = Running::start().await;
    let banner = banner_fixture(&running, 4);
    let mut terminal = running
        .runtime
        .attach(
            &binding(),
            TerminalSize::new(100, 30).unwrap(),
            running.lease(Duration::from_secs(30)),
        )
        .await
        .unwrap();
    // Let the unsolicited banner reach the two-frame output queue before
    // issuing commands. No terminal output is consumed until both are acknowledged.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let write = tokio::time::timeout(Duration::from_secs(1), terminal.write(&[27, 0, 254])).await;
    assert!(
        matches!(write, Ok(Ok(()))),
        "output backpressure blocked the write acknowledgement"
    );
    let resize = tokio::time::timeout(
        Duration::from_secs(1),
        terminal.resize(TerminalSize::new(81, 25).unwrap()),
    )
    .await;
    assert!(
        matches!(resize, Ok(Ok(()))),
        "output backpressure blocked the resize acknowledgement"
    );
    // The actual SSH peer must receive input and resize, not only local enqueue.
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let delivered = {
                let state = running.fake.0.lock().unwrap();
                state.ssh.bytes == [27, 0, 254] && state.ssh.size == (81, 25)
            };
            if delivered {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let mut expected = banner.concat();
    expected.extend_from_slice(&[27, 0, 254]);
    let actual = tokio::time::timeout(Duration::from_secs(3), async {
        let mut actual = Vec::new();
        while actual.len() < expected.len() {
            let TerminalOutput::Data(bytes) = terminal.read().await.unwrap().unwrap() else {
                continue;
            };
            assert!(!bytes.is_empty() && bytes.len() <= MAX_CHUNK_BYTES);
            actual.extend(bytes);
        }
        actual
    })
    .await
    .unwrap();
    assert_eq!(actual, expected);
    terminal.detach().await.unwrap();
    wait_for_revoke(&running.fake).await;
    assert_eq!(
        running.fake.0.lock().unwrap().ssh.subsystems,
        ["openshell-main"]
    );
}

#[tokio::test]
async fn stalled_terminal_output_cancel_revokes_without_main_process_eof() {
    for explicit_detach in [true, false] {
        let running = Running::start().await;
        banner_fixture(&running, 16);
        let terminal = running
            .runtime
            .attach(
                &binding(),
                TerminalSize::new(100, 30).unwrap(),
                running.lease(Duration::from_secs(30)),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        if explicit_detach {
            tokio::time::timeout(Duration::from_secs(3), terminal.detach())
                .await
                .unwrap()
                .unwrap();
        } else {
            drop(terminal);
        }
        wait_for_revoke(&running.fake).await;
        assert!(running.fake.0.lock().unwrap().ssh.bytes.is_empty());
    }
}

#[tokio::test]
async fn stalled_terminal_output_expires_without_reads_or_process_stop() {
    let running = Running::start().await;
    banner_fixture(&running, 16);
    let terminal = running
        .runtime
        .attach(
            &binding(),
            TerminalSize::new(100, 30).unwrap(),
            running.lease(Duration::from_millis(1200)),
        )
        .await
        .unwrap();
    // No read, write or resize drives the worker's absolute lease deadline.
    wait_for_revoke(&running.fake).await;
    assert!(matches!(
        terminal.write(&[1]).await,
        Err(RuntimeFailure::LeaseExpired)
    ));
    assert!(matches!(
        terminal.detach().await,
        Err(RuntimeFailure::LeaseExpired)
    ));
    assert!(running.fake.0.lock().unwrap().ssh.bytes.is_empty());
}
