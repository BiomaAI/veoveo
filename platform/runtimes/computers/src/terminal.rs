use crate::{
    AttachmentLease, Binding, MAX_CHUNK_BYTES, Observation, OpenShellRuntime, Phase, Result,
    RuntimeFailure, TerminalOutput, TerminalSize,
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

// Only closed categories cross the diagnostic boundary. SDK messages, requests,
// credentials, host names and SSH payloads never enter this formatter.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TerminalStage {
    SessionMint,
    SessionAdmission,
    ForwardConnect,
    ForwardAdmission,
    ForwardRead,
    ForwardWrite,
    SshConnect,
    SshAuthenticate,
    SshChannel,
    Pty,
    Shell,
    Continuity,
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum TerminalCause {
    Rpc(tonic::Code),
    Io(std::io::ErrorKind),
    Ssh,
    Transport,
    Deadline,
    Refused,
    EndOfStream,
    InvalidResponse,
}
pub(crate) fn diagnose(stage: TerminalStage, cause: TerminalCause) {
    eprintln!("computer terminal: {}", diagnostic(stage, cause));
}
fn diagnostic(stage: TerminalStage, cause: TerminalCause) -> String {
    // Explicitly unpack payloads so only the SDK's closed code/kind is rendered.
    let cause = match cause {
        TerminalCause::Rpc(code) => format!("Rpc({code:?})"),
        TerminalCause::Io(kind) => format!("Io({kind:?})"),
        other => format!("{other:?}"),
    };
    format!("stage={stage:?} cause={cause}")
}
fn ssh_failure(stage: TerminalStage, error: russh::Error) -> RuntimeFailure {
    let cause = match error {
        russh::Error::IO(error) => TerminalCause::Io(error.kind()),
        _ => TerminalCause::Ssh,
    };
    diagnose(stage, cause);
    RuntimeFailure::TerminalFailed
}
fn rpc_failure(stage: TerminalStage, error: tonic::Status) -> RuntimeFailure {
    diagnose(stage, TerminalCause::Rpc(error.code()));
    RuntimeFailure::TerminalFailed
}

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
#[derive(Clone)]
pub struct TerminalInput {
    input: mpsc::Sender<Command>,
    lease: AttachmentLease,
}
pub struct Terminal {
    input: mpsc::Sender<Command>,
    output: mpsc::Receiver<Result<TerminalOutput>>,
    cancel: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<Result<()>>>,
    finished: Option<Result<()>>,
    resource: String,
    instance: String,
    lease: AttachmentLease,
}
impl Terminal {
    pub fn input(&self) -> TerminalInput {
        TerminalInput {
            input: self.input.clone(),
            lease: self.lease.clone(),
        }
    }
    pub fn sandbox_id(&self) -> &str {
        &self.resource
    }
    pub fn main_process_instance_id(&self) -> &str {
        &self.instance
    }
    pub fn expires_at(&self) -> Result<SystemTime> {
        self.lease.expires_at()
    }
    pub async fn read(&mut self) -> Result<Option<TerminalOutput>> {
        let received = tokio::select! {
            biased;
            _ = self.lease.closed() => return Err(RuntimeFailure::LeaseExpired),
            received = self.output.recv() => received,
        };
        match received {
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
        self.input().write(bytes).await
    }
    pub async fn resize(&self, size: TerminalSize) -> Result<()> {
        self.input().resize(size).await
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
impl TerminalInput {
    pub async fn write(&self, bytes: &[u8]) -> Result<()> {
        self.lease.check()?;
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
        self.lease.check()?;
        let (tx, rx) = oneshot::channel();
        self.input
            .send(Command::Resize(size, tx))
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)?;
        rx.await.map_err(|_| RuntimeFailure::TerminalFailed)?
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

pub(crate) struct SessionToken {
    token: Option<Zeroizing<String>>,
    client: Client,
}
impl SessionToken {
    pub(crate) fn into_token(mut self) -> Zeroizing<String> {
        self.token.take().expect("issued token is owned")
    }
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
                ..Default::default()
            },
            5,
        )),
    )
    .await
    .map_err(|_| RuntimeFailure::TerminalFailed)?
    .map_err(|_| RuntimeFailure::TerminalFailed)?;
    Ok(())
}

pub(crate) async fn issue_session(
    client: Client,
    sandbox: String,
    workspace: String,
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
                rpc.create_ssh_session(request(
                    api::CreateSshSessionRequest {
                        sandbox,
                        workspace_scope: crate::client::workspace_scope(&workspace),
                    },
                    10,
                )),
            )
            .await
            .map_err(|_| {
                diagnose(TerminalStage::SessionMint, TerminalCause::Deadline);
                RuntimeFailure::TerminalFailed
            })?
            .map_err(|error| rpc_failure(TerminalStage::SessionMint, error))?
            .into_inner();
            if response.token.is_empty() || response.token.len() > 4096 {
                diagnose(TerminalStage::SessionMint, TerminalCause::InvalidResponse);
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

impl OpenShellRuntime {
    pub async fn attach(
        &self,
        binding: &Binding,
        size: TerminalSize,
        lease: AttachmentLease,
    ) -> Result<Terminal> {
        lease.check()?;
        let (input, commands) = mpsc::channel(1);
        let (output_tx, output) = mpsc::channel(2);
        let (ready_tx, ready_rx) = oneshot::channel();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let mut cancel = AttachCancel(Some(cancel_tx));
        let runtime = self.clone();
        let binding = binding.clone();
        let worker_lease = lease.clone();
        let worker = tokio::spawn(async move {
            terminal_worker(
                runtime,
                binding,
                size,
                worker_lease,
                commands,
                output_tx,
                ready_tx,
                cancel_rx,
            )
            .await
        });
        let observed = ready_rx
            .await
            .map_err(|_| RuntimeFailure::TerminalFailed)??;
        Ok(Terminal {
            input,
            output,
            cancel: cancel.0.take(),
            worker: Some(worker),
            finished: None,
            resource: observed.sandbox_id,
            instance: observed.main_process_instance_id,
            lease,
        })
    }
}
type SshChannel = russh::Channel<client::Msg>;
async fn channel_success(channel: &mut SshChannel, stage: TerminalStage) -> Result<()> {
    match channel.wait().await {
        Some(ChannelMsg::Success) => Ok(()),
        Some(_) => {
            diagnose(stage, TerminalCause::Refused);
            Err(RuntimeFailure::TerminalFailed)
        }
        None => {
            diagnose(stage, TerminalCause::EndOfStream);
            Err(RuntimeFailure::TerminalFailed)
        }
    }
}
struct Attached {
    client: client::Handle<HostKey>,
    channel: SshChannel,
    current: Observation,
}
async fn setup(
    runtime: &OpenShellRuntime,
    binding: &Binding,
    size: TerminalSize,
    token: &mut SessionToken,
    bridges: &mut JoinSet<Result<()>>,
    lease: &AttachmentLease,
) -> Result<Attached> {
    let current = runtime
        .get(binding)
        .await?
        .ok_or(RuntimeFailure::NotFound)?;
    if current.phase != Phase::Ready || current.main_process_instance_id.is_empty() {
        return Err(RuntimeFailure::InvalidState);
    }
    let (session, issued_token) = issue_session(
        runtime.client.clone(),
        binding.name(),
        runtime.workspace.clone(),
    )
    .await?;
    *token = issued_token;
    if session.sandbox_id != current.sandbox_id
        || crate::client::timestamp_millis(session.expiration_time.as_ref())
            .is_none_or(|expiry| expiry <= 0)
    {
        diagnose(
            TerminalStage::SessionAdmission,
            TerminalCause::InvalidResponse,
        );
        return Err(RuntimeFailure::TerminalFailed);
    }
    let admission_expires = UNIX_EPOCH
        + Duration::from_millis(
            crate::client::timestamp_millis(session.expiration_time.as_ref()).unwrap() as u64,
        );
    let duration = admission_expires
        .duration_since(SystemTime::now())
        .ok()
        .filter(|duration| !duration.is_zero())
        .ok_or(RuntimeFailure::LeaseExpired)?
        .min(Duration::from_secs(30));
    let (ssh, relay) = tokio::io::duplex(MAX_CHUNK_BYTES * 2);
    let forward_token = token.token.as_ref().unwrap().to_string();
    let stub = runtime.clone();
    let id = current.sandbox_id.clone();
    bridges.spawn(forward(
        stub,
        binding.name(),
        id,
        Zeroizing::new(forward_token),
        relay,
        lease.clone(),
        admission_expires,
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
        .map_err(|error| ssh_failure(TerminalStage::SshConnect, error))?;
        if !matches!(
            client
                .authenticate_none("sandbox")
                .await
                .map_err(|error| ssh_failure(TerminalStage::SshAuthenticate, error))?,
            client::AuthResult::Success
        ) {
            diagnose(TerminalStage::SshAuthenticate, TerminalCause::Refused);
            return Err(RuntimeFailure::TerminalFailed);
        }
        let mut channel = client
            .channel_open_session()
            .await
            .map_err(|error| ssh_failure(TerminalStage::SshChannel, error))?;
        channel
            .request_pty(true, "xterm-256color", size.cols, size.rows, 0, 0, &[])
            .await
            .map_err(|error| ssh_failure(TerminalStage::Pty, error))?;
        channel_success(&mut channel, TerminalStage::Pty).await?;
        channel
            .request_shell(true)
            .await
            .map_err(|error| ssh_failure(TerminalStage::Shell, error))?;
        channel_success(&mut channel, TerminalStage::Shell).await?;
        if !current.same_process(
            &runtime
                .get(binding)
                .await?
                .ok_or(RuntimeFailure::TerminalFailed)?,
        ) {
            diagnose(TerminalStage::Continuity, TerminalCause::InvalidResponse);
            return Err(RuntimeFailure::TerminalFailed);
        }
        Ok(Attached {
            client,
            channel,
            current,
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
    lease: AttachmentLease,
    mut commands: mpsc::Receiver<Command>,
    output: mpsc::Sender<Result<TerminalOutput>>,
    ready: oneshot::Sender<Result<Observation>>,
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
        _=lease.closed() => Err(RuntimeFailure::LeaseExpired),
        result=tokio::time::timeout(Duration::from_secs(45),setup(&runtime,&binding,size,&mut token,&mut bridges,&lease)) => result.map_err(|_|RuntimeFailure::TerminalFailed).and_then(|r|r),
    };
    let result = match setup_result {
        Err(error) => {
            let _ = ready.send(Err(error));
            Err(error)
        }
        Ok(attached) => {
            let (mut reader, writer) = attached.channel.split();
            let result = if ready.send(Ok(attached.current)).is_err() {
                Ok(())
            } else {
                // Each direction owns its pending I/O. In particular, a full
                // bounded output queue must not park the write/resize loop.
                // Neither pump is spawned; cancellation ends pending I/O before
                // the owned channel close and independently bounded token cleanup.
                let work = async {
                    tokio::select! {
                        _=bridges.join_next()=>Err(RuntimeFailure::TerminalFailed),
                        result=pump_input(&writer, &mut commands)=>result,
                        result=pump_output(&mut reader, &output)=>result,
                    }
                };
                let result = tokio::select! {
                    biased;
                    _=&mut cancel=>Ok(()),
                    _=lease.closed()=>Err(RuntimeFailure::LeaseExpired),
                    result=work=>result,
                };
                result
            };
            // Closing this stock SSH channel ends its fresh shell, not the Computer.
            // Keep the authenticated bridge alive until the close is delivered.
            let mut ssh = attached.client;
            let closed = if lease.check().is_err() {
                // The forwarding lease has already closed its transport. Do not
                // retain expired authority while waiting for remote teardown.
                Err(RuntimeFailure::LeaseExpired)
            } else {
                tokio::time::timeout(Duration::from_secs(2), async {
                    writer.close().await?;
                    ssh.disconnect(russh::Disconnect::ByApplication, "", "en")
                        .await?;
                    let mut receiving = true;
                    loop {
                        tokio::select! {
                            result = &mut ssh => return match result {
                                Ok(()) | Err(russh::Error::Disconnect) => Ok(()),
                                Err(error) => Err(error),
                            },
                            message = reader.wait(), if receiving => {
                                // Drain bounded channel messages so queued output
                                // cannot park the SSH task's teardown. Store none.
                                if message.is_none() { receiving = false; }
                            }
                        }
                    }
                })
                .await
                .map_err(|_| RuntimeFailure::TerminalFailed)
                .and_then(|result| result.map_err(|_| RuntimeFailure::TerminalFailed))
            };
            // Waiting for the owned SSH task flushes queued close/disconnect bytes.
            // It does not establish remote process exit; failure stays unresolved.
            bridges.abort_all();
            result.and(closed)
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
    writer: &russh::ChannelWriteHalf<client::Msg>,
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
fn admit_forward_authority(lease: &AttachmentLease, expiration: SystemTime) -> Result<()> {
    lease.check()?;
    if SystemTime::now() >= expiration {
        return Err(RuntimeFailure::LeaseExpired);
    }
    Ok(())
}
async fn forward(
    runtime: OpenShellRuntime,
    runtime_tunnel_name: String,
    id: String,
    token: Zeroizing<String>,
    relay: tokio::io::DuplexStream,
    lease: AttachmentLease,
    admission_expires: SystemTime,
) -> Result<()> {
    let (mut client, _transport) =
        crate::attachment_transport::connect(&runtime.endpoint, &runtime.address).await?;
    let (reader, mut writer) = tokio::io::split(relay);
    let init = api::TcpForwardFrame {
        payload: Some(api::tcp_forward_frame::Payload::Init(api::TcpForwardInit {
            sandbox: runtime_tunnel_name.clone(),
            workspace: runtime.workspace.clone(),
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
    let request = tonic::Request::new(stream::once(async move { init }).chain(data));
    // A credential admits this tunnel. The enclosing worker owns the renewable
    // authority timer and aborts the bridge; no fixed gRPC deadline ends live work.
    // The gateway admits an authenticated reconnect for at most 15 seconds.
    // This 20-second RPC admission budget includes transport overhead, while
    // setup's credential deadline and the owning lease can end it sooner.
    admit_forward_authority(&lease, admission_expires)?;
    let admission_budget = admission_expires
        .duration_since(SystemTime::now())
        .map_err(|_| RuntimeFailure::LeaseExpired)?
        .min(Duration::from_secs(20));
    let mut response = lease
        .enforce(async {
            tokio::time::timeout(admission_budget, client.forward_tcp(request))
                .await
                .map_err(|_| {
                    diagnose(TerminalStage::ForwardAdmission, TerminalCause::Deadline);
                    RuntimeFailure::TerminalFailed
                })?
                .map_err(|error| rpc_failure(TerminalStage::ForwardAdmission, error))
        })
        .await?
        .into_inner();
    // Reconnect admission may have consumed the authority window. A returned
    // stream cannot revive expired/revoked access or an expired mint credential.
    admit_forward_authority(&lease, admission_expires)?;
    while let Some(frame) = response
        .message()
        .await
        .map_err(|error| rpc_failure(TerminalStage::ForwardRead, error))?
    {
        let bytes = forward_data(frame)?;
        writer.write_all(&bytes).await.map_err(|error| {
            diagnose(TerminalStage::ForwardWrite, TerminalCause::Io(error.kind()));
            RuntimeFailure::TerminalFailed
        })?;
    }
    diagnose(TerminalStage::ForwardRead, TerminalCause::EndOfStream);
    Err(RuntimeFailure::TerminalFailed)
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn awaited_forward_admission_cannot_revive_expired_or_revoked_authority() {
        let (authority, lease) =
            crate::LeaseAuthority::issue(tokio::time::Instant::now(), Duration::from_secs(30))
                .unwrap();
        assert_eq!(
            admit_forward_authority(&lease, SystemTime::now() + Duration::from_secs(30)),
            Ok(())
        );
        assert_eq!(
            admit_forward_authority(&lease, UNIX_EPOCH),
            Err(RuntimeFailure::LeaseExpired)
        );
        authority.revoke();
        assert_eq!(
            admit_forward_authority(&lease, SystemTime::now() + Duration::from_secs(30)),
            Err(RuntimeFailure::LeaseExpired)
        );
    }
    #[test]
    fn sdk_diagnostics_exclude_messages_and_payloads() {
        let status = tonic::Status::permission_denied("Bearer private-token query=secret");
        let text = diagnostic(
            TerminalStage::ForwardAdmission,
            TerminalCause::Rpc(status.code()),
        );
        assert_eq!(text, "stage=ForwardAdmission cause=Rpc(PermissionDenied)");
        let error =
            std::io::Error::new(std::io::ErrorKind::ConnectionReset, "private-key and bytes");
        assert_eq!(
            diagnostic(TerminalStage::ForwardWrite, TerminalCause::Io(error.kind())),
            "stage=ForwardWrite cause=Io(ConnectionReset)"
        );
    }
}
