use crate::{
    Binding, MAX_CHUNK_BYTES, Observation, OpenShellRuntime, Phase, Result, RuntimeFailure,
    TerminalOutput, TerminalSize,
    client::{Client, request},
    protocol::v1 as api,
    terminal_output::pump_output,
};
use futures::{StreamExt, stream};
use russh::{
    ChannelMsg, client,
    keys::{HashAlg, PublicKeyOrCertificate},
};
use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Semaphore, mpsc, oneshot},
    task::{JoinHandle, JoinSet},
};
use zeroize::Zeroizing;

// Orphan-response cleanup retains admission until its bounded revoke finishes.
static SESSION_ISSUANCES: Semaphore = Semaphore::const_new(32);

struct HostKey {
    expected: String,
}
impl client::Handler for HostKey {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> std::result::Result<bool, Self::Error> {
        // No unauthenticated leg: the private duplex reaches only the mTLS
        // gateway's exact sandbox tunnel. Honor its optional additional pin.
        Ok(self.expected.is_empty()
            || key.public_key().fingerprint(HashAlg::Sha256).to_string() == self.expected)
    }
}
enum Command {
    Write(Vec<u8>, oneshot::Sender<Result<()>>),
    Resize(TerminalSize, oneshot::Sender<Result<()>>),
}
pub struct Terminal {
    input: mpsc::Sender<Command>,
    output: mpsc::Receiver<Result<TerminalOutput>>,
    cancel: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<Result<()>>>,
    finished: Option<Result<()>>,
    instance: String,
    expires: SystemTime,
}
impl Terminal {
    pub fn main_process_instance_id(&self) -> &str {
        &self.instance
    }
    pub fn expires_at(&self) -> SystemTime {
        self.expires
    }
    pub async fn read(&mut self) -> Result<Option<TerminalOutput>> {
        match self.output.recv().await {
            Some(result) => result.map(Some),
            None => {
                if let Some(worker) = self.worker.take() {
                    self.finished = Some(worker.await.map_err(|_| RuntimeFailure::TerminalFailed)?);
                }
                self.finished.unwrap_or(Ok(())).map(|()| None)
            }
        }
    }
    pub async fn write(&self, bytes: &[u8]) -> Result<()> {
        if bytes.is_empty() || bytes.len() > MAX_CHUNK_BYTES {
            return Err(RuntimeFailure::TerminalBounds);
        }
        let (tx, rx) = oneshot::channel();
        self.input
            .send(Command::Write(bytes.to_vec(), tx))
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?;
        rx.await.map_err(|_| RuntimeFailure::TerminalFailed)?
    }
    pub async fn resize(&self, size: TerminalSize) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.input
            .send(Command::Resize(size, tx))
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?;
        rx.await.map_err(|_| RuntimeFailure::TerminalFailed)?
    }
    pub async fn detach(mut self) -> Result<()> {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
        if let Some(worker) = self.worker.take() {
            worker.await.map_err(|_| RuntimeFailure::TerminalFailed)??;
        }
        self.finished.unwrap_or(Ok(()))
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }
}
// Protect cancellation while the public attach future is still waiting.
struct AttachCancel(Option<oneshot::Sender<()>>);
impl Drop for AttachCancel {
    fn drop(&mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

struct SessionToken {
    token: Option<Zeroizing<String>>,
    client: Client,
}
impl SessionToken {
    async fn revoke(&mut self) -> Result<()> {
        let Some(token) = self.token.take() else {
            return Ok(());
        };
        revoke(self.client.clone(), token).await
    }
}
impl Drop for SessionToken {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            // Fallback for canceled attach tasks; no token is logged or placed
            // in arguments. Runtime shutdown still relies on provider expiry.
            let client = self.client.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = revoke(client, token).await;
                });
            }
        }
    }
}
async fn revoke(mut client: Client, token: Zeroizing<String>) -> Result<()> {
    tokio::time::timeout(
        Duration::from_secs(5),
        client.revoke_ssh_session(request(
            api::RevokeSshSessionRequest {
                token: token.to_string(),
            },
            5,
        )),
    )
    .await
    .map_err(|_| RuntimeFailure::TerminalFailed)?
    .map_err(|_| RuntimeFailure::TerminalFailed)?;
    Ok(())
}

async fn issue_session(
    client: Client,
    sandbox_id: String,
) -> Result<(api::CreateSshSessionResponse, SessionToken)> {
    let permit = SESSION_ISSUANCES
        .try_acquire()
        .map_err(|_| RuntimeFailure::TerminalFailed)?;
    let (deliver, receive) = oneshot::channel();
    // Once dispatched, minting can succeed remotely before the response is
    // visible locally. Cancelling attach must not discard that response. This
    // owner drains one bounded RPC, then hands off a guarded token or revokes it.
    tokio::spawn(async move {
        let _permit = permit;
        if deliver.is_closed() {
            return;
        }
        let result = async {
            let mut rpc = client.clone();
            let mut response = tokio::time::timeout(
                Duration::from_secs(10),
                rpc.create_ssh_session(request(api::CreateSshSessionRequest { sandbox_id }, 10)),
            )
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?
            .map_err(|_| RuntimeFailure::TerminalFailed)?
            .into_inner();
            if response.token.is_empty() || response.token.len() > 4096 {
                return Err(RuntimeFailure::TerminalFailed);
            }
            let token = SessionToken {
                token: Some(Zeroizing::new(std::mem::take(&mut response.token))),
                client,
            };
            Ok((response, token))
        }
        .await;
        if let Err(Ok((_response, mut token))) = deliver.send(result) {
            // Caller cancellation is not cleanup cancellation. Revoke has its
            // own five-second deadline and never affects the main process.
            let _ = token.revoke().await;
        }
        // If send succeeded but the receiver drops before collecting it,
        // SessionToken::drop still owns bounded revocation of the queued value.
    });
    receive.await.map_err(|_| RuntimeFailure::TerminalFailed)?
}

fn remaining(expires: SystemTime) -> Result<Duration> {
    expires
        .duration_since(SystemTime::now())
        .ok()
        .filter(|d| !d.is_zero() && *d <= Duration::from_secs(900))
        .ok_or(RuntimeFailure::LeaseExpired)
}

impl OpenShellRuntime {
    pub async fn attach(
        &self,
        binding: &Binding,
        size: TerminalSize,
        expires: SystemTime,
    ) -> Result<Terminal> {
        remaining(expires)?;
        let (input, commands) = mpsc::channel(1);
        let (output_tx, output) = mpsc::channel(2);
        let (ready_tx, ready_rx) = oneshot::channel();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let mut cancel = AttachCancel(Some(cancel_tx));
        let runtime = self.clone();
        let binding = binding.clone();
        let worker = tokio::spawn(async move {
            terminal_worker(
                runtime, binding, size, expires, commands, output_tx, ready_tx, cancel_rx,
            )
            .await
        });
        let (instance, expires) = ready_rx
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)??;
        Ok(Terminal {
            input,
            output,
            cancel: cancel.0.take(),
            worker: Some(worker),
            finished: None,
            instance,
            expires,
        })
    }
}
type SshChannel = russh::Channel<client::Msg>;
async fn channel_success(channel: &mut SshChannel) -> Result<()> {
    match channel.wait().await {
        Some(ChannelMsg::Success) => Ok(()),
        _ => Err(RuntimeFailure::TerminalFailed),
    }
}
struct Attached {
    client: client::Handle<HostKey>,
    channel: SshChannel,
    current: Observation,
    expires: SystemTime,
}
async fn setup(
    runtime: &OpenShellRuntime,
    binding: &Binding,
    size: TerminalSize,
    expires: SystemTime,
    token: &mut SessionToken,
    bridges: &mut JoinSet<Result<()>>,
) -> Result<Attached> {
    let current = runtime
        .get(binding)
        .await?
        .ok_or(RuntimeFailure::NotFound)?;
    if current.phase != Phase::Ready || current.main_process_instance_id.is_empty() {
        return Err(RuntimeFailure::InvalidState);
    }
    remaining(expires)?;
    let (session, issued_token) =
        issue_session(runtime.client.clone(), current.sandbox_id.clone()).await?;
    *token = issued_token;
    if session.sandbox_id != current.sandbox_id || session.expires_at_ms <= 0 {
        return Err(RuntimeFailure::TerminalFailed);
    }
    let expires = expires.min(UNIX_EPOCH + Duration::from_millis(session.expires_at_ms as u64));
    let duration = remaining(expires)?;
    let (ssh, relay) = tokio::io::duplex(MAX_CHUNK_BYTES * 2);
    let forward_token = token.token.as_ref().unwrap().to_string();
    let stub = runtime.client.clone();
    let id = current.sandbox_id.clone();
    bridges.spawn(forward(
        stub,
        id,
        Zeroizing::new(forward_token),
        relay,
        duration,
    ));
    tokio::time::timeout(duration, async {
        let config = client::Config {
            window_size: (MAX_CHUNK_BYTES * 4) as u32,
            maximum_packet_size: MAX_CHUNK_BYTES as u32,
            channel_buffer_size: 2,
            keepalive_interval: Some(Duration::from_secs(20)),
            keepalive_max: 2,
            ..Default::default()
        };
        let mut client = client::connect_stream(
            Arc::new(config),
            ssh,
            HostKey {
                expected: session.host_key_fingerprint,
            },
        )
        .await
        .map_err(|_| RuntimeFailure::TerminalFailed)?;
        if !matches!(
            client
                .authenticate_none("sandbox")
                .await
                .map_err(|_| RuntimeFailure::TerminalFailed)?,
            client::AuthResult::Success
        ) {
            return Err(RuntimeFailure::TerminalFailed);
        }
        let mut channel = client
            .channel_open_session()
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?;
        channel
            .request_pty(true, "xterm-256color", size.cols, size.rows, 0, 0, &[])
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?;
        channel_success(&mut channel).await?;
        channel
            .set_env(true, "OPENSHELL_MAIN_EVENTS", "1")
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?;
        channel_success(&mut channel).await?;
        channel
            .request_subsystem(true, "openshell-main")
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?;
        channel_success(&mut channel).await?;
        if !current.same_process(
            &runtime
                .get(binding)
                .await?
                .ok_or(RuntimeFailure::TerminalFailed)?,
        ) {
            return Err(RuntimeFailure::TerminalFailed);
        }
        Ok(Attached {
            client,
            channel,
            current,
            expires,
        })
    })
    .await
    .map_err(|_| RuntimeFailure::LeaseExpired)?
}
#[allow(clippy::too_many_arguments)]
async fn terminal_worker(
    runtime: OpenShellRuntime,
    binding: Binding,
    size: TerminalSize,
    expires: SystemTime,
    mut commands: mpsc::Receiver<Command>,
    output: mpsc::Sender<Result<TerminalOutput>>,
    ready: oneshot::Sender<Result<(String, SystemTime)>>,
    mut cancel: oneshot::Receiver<()>,
) -> Result<()> {
    let mut token = SessionToken {
        token: None,
        client: runtime.client.clone(),
    };
    let mut bridges = JoinSet::new();
    let setup_result = tokio::select! {
        biased;
        _=&mut cancel => Err(RuntimeFailure::TerminalFailed),
        result=tokio::time::timeout(remaining(expires)?.min(Duration::from_secs(45)),setup(&runtime,&binding,size,expires,&mut token,&mut bridges)) => result.map_err(|_|RuntimeFailure::TerminalFailed).and_then(|r|r),
    };
    let result = match setup_result {
        Err(error) => {
            let _ = ready.send(Err(error));
            Err(error)
        }
        Ok(attached) => {
            let expires = attached.expires;
            let (reader, writer) = attached.channel.split();
            if ready
                .send(Ok((attached.current.main_process_instance_id, expires)))
                .is_err()
            {
                Ok(())
            } else {
                // Each direction owns its pending I/O. In particular, a full
                // bounded output queue must not park the write/resize loop.
                // Neither pump is spawned: cancellation drops both channel halves
                // before disconnect and the independently bounded token cleanup.
                let work = async {
                    tokio::select! {
                        _=bridges.join_next()=>Err(RuntimeFailure::TerminalFailed),
                        result=pump_input(writer, &mut commands)=>result,
                        result=pump_output(reader, &output)=>result,
                    }
                };
                let result = match remaining(expires) {
                    Err(error) => {
                        drop(work);
                        Err(error)
                    }
                    Ok(duration) => {
                        let deadline = tokio::time::Instant::now() + duration;
                        tokio::select! {
                            biased;
                            _=&mut cancel=>Ok(()),
                            _=tokio::time::sleep_until(deadline)=>Err(RuntimeFailure::LeaseExpired),
                            result=work=>result,
                        }
                    }
                };
                // SSH disconnect releases the input lease. Do not send channel
                // stdin EOF, a signal, kill, exec, or a fresh shell request.
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    attached
                        .client
                        .disconnect(russh::Disconnect::ByApplication, "", "en"),
                )
                .await;
                result
            }
        }
    };
    bridges.abort_all();
    while bridges.join_next().await.is_some() {}
    let revoked = token.revoke().await;
    let result = result.and(revoked);
    if let Err(error) = result {
        let _ = output.try_send(Err(error));
    }
    result
}

async fn pump_input(
    writer: russh::ChannelWriteHalf<client::Msg>,
    commands: &mut mpsc::Receiver<Command>,
) -> Result<()> {
    while let Some(command) = commands.recv().await {
        let (result, ack) = match command {
            Command::Write(bytes, ack) => (writer.data(std::io::Cursor::new(bytes)).await, ack),
            Command::Resize(size, ack) => {
                (writer.window_change(size.cols, size.rows, 0, 0).await, ack)
            }
        };
        let result = result.map_err(|_| RuntimeFailure::TerminalFailed);
        let _ = ack.send(result);
        result?;
    }
    Ok(())
}

pub(crate) fn forward_data(frame: api::TcpForwardFrame) -> Result<Vec<u8>> {
    match frame.payload {
        Some(api::tcp_forward_frame::Payload::Data(bytes)) if bytes.len() <= MAX_CHUNK_BYTES => {
            Ok(bytes)
        }
        _ => Err(RuntimeFailure::TerminalBounds),
    }
}
async fn forward(
    mut client: Client,
    id: String,
    token: Zeroizing<String>,
    relay: tokio::io::DuplexStream,
    duration: Duration,
) -> Result<()> {
    let (reader, mut writer) = tokio::io::split(relay);
    let init = api::TcpForwardFrame {
        payload: Some(api::tcp_forward_frame::Payload::Init(api::TcpForwardInit {
            sandbox_id: id.clone(),
            service_id: format!("ssh-proxy:{id}"),
            target: Some(api::tcp_forward_init::Target::Ssh(api::SshRelayTarget {})),
            authorization_token: token.to_string(),
        })),
    };
    let data = stream::unfold(reader, |mut reader| async move {
        let mut bytes = vec![0; MAX_CHUNK_BYTES];
        match reader.read(&mut bytes).await {
            Ok(n) if n > 0 => {
                bytes.truncate(n);
                Some((
                    api::TcpForwardFrame {
                        payload: Some(api::tcp_forward_frame::Payload::Data(bytes)),
                    },
                    reader,
                ))
            }
            _ => None,
        }
    });
    let mut request = tonic::Request::new(stream::once(async move { init }).chain(data));
    request.set_timeout(duration);
    let mut response = client
        .forward_tcp(request)
        .await
        .map_err(|_| RuntimeFailure::TerminalFailed)?
        .into_inner();
    while let Some(frame) = response
        .message()
        .await
        .map_err(|_| RuntimeFailure::TerminalFailed)?
    {
        let bytes = forward_data(frame)?;
        writer
            .write_all(&bytes)
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?;
    }
    Err(RuntimeFailure::TerminalFailed)
}
