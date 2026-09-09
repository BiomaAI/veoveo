//! One native exit event establishes completion; canceled observation is unknown.
use crate::{
    Binding, MAX_CHUNK_BYTES, OpenShellRuntime, Phase, Result, RuntimeFailure, client::request,
    models::canonical_path, protocol::v1 as api,
};
use futures::stream;
use std::{future::Future, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::mpsc,
};
#[derive(Clone)]
pub struct ExecIntent {
    command: Vec<String>,
    workdir: String,
    timeout_seconds: u32,
    maximum_output_bytes: usize,
    stdin: Vec<u8>,
}
impl ExecIntent {
    pub fn new(
        command: Vec<String>,
        workdir: String,
        timeout_seconds: u32,
        maximum_output_bytes: usize,
        stdin: Vec<u8>,
    ) -> Result<Self> {
        if command.is_empty()
            || command.len() > 32
            || command
                .iter()
                .any(|s| s.is_empty() || s.len() > 8192 || s.contains('\0'))
            || command.iter().map(String::len).sum::<usize>() > 32768
            || !canonical_path(&command[0])
            || workdir.len() > 1024
            || !canonical_path(&workdir)
            || !(workdir == "/sandbox" || workdir.starts_with("/sandbox/"))
            || !(1..=7200).contains(&timeout_seconds)
            || !(1..=64 * 1024 * 1024).contains(&maximum_output_bytes)
            || stdin.len() > MAX_CHUNK_BYTES
        {
            return Err(RuntimeFailure::InvalidExecution);
        }
        Ok(Self {
            command,
            workdir,
            timeout_seconds,
            maximum_output_bytes,
            stdin,
        })
    }
}
/// An already-authorized borrowed local source. No filenames or URLs cross this API.
pub struct ExecInput<'a> {
    source: &'a mut (dyn AsyncRead + Unpin + Send),
    byte_count: u64,
}
impl<'a> ExecInput<'a> {
    pub fn new(source: &'a mut (dyn AsyncRead + Unpin + Send), byte_count: u64) -> Result<Self> {
        if !(1..=1024 * 1024 * 1024).contains(&byte_count) {
            return Err(RuntimeFailure::InvalidExecution);
        }
        Ok(Self { source, byte_count })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}
pub struct ExecChunk {
    pub stream: OutputStream,
    pub data: Vec<u8>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
}
struct Events {
    exit_code: Option<i32>,
    stdout: usize,
    stderr: usize,
    count: u32,
    maximum: usize,
}
impl Events {
    fn accept(&mut self, event: api::ExecSandboxEvent) -> Result<Option<ExecChunk>> {
        self.count += 1;
        if self.count > 65536 || self.exit_code.is_some() {
            return Err(RuntimeFailure::ExecutionUnknown);
        }
        use api::exec_sandbox_event::Payload;
        let (stream, data) = match event.payload {
            Some(Payload::Exit(exit)) => {
                if !(0..=255).contains(&exit.exit_code) || exit.exit_code == 124 {
                    return Err(RuntimeFailure::ExecutionUnknown);
                }
                self.exit_code = Some(exit.exit_code);
                return Ok(None);
            }
            Some(Payload::Stdout(out)) => (OutputStream::Stdout, out.data),
            Some(Payload::Stderr(out)) => (OutputStream::Stderr, out.data),
            None => return Err(RuntimeFailure::ExecutionUnknown),
        };
        if data.len() > MAX_CHUNK_BYTES {
            return Err(RuntimeFailure::ExecutionUnknown);
        }
        match stream {
            OutputStream::Stdout => self.stdout += data.len(),
            OutputStream::Stderr => self.stderr += data.len(),
        }
        if self.stdout + self.stderr > self.maximum {
            return Err(RuntimeFailure::ExecutionUnknown);
        }
        Ok((!data.is_empty()).then_some(ExecChunk { stream, data }))
    }
    fn result(&self) -> Result<ExecResult> {
        Ok(ExecResult {
            exit_code: self.exit_code.ok_or(RuntimeFailure::ExecutionUnknown)?,
            stdout_bytes: self.stdout,
            stderr_bytes: self.stderr,
        })
    }
}
impl OpenShellRuntime {
    /// Call only for an authorized, durably fenced installation build worker.
    /// Args must contain no credentials: the pinned gateway logs command previews.
    pub async fn execute<F, Fut>(
        &self,
        binding: &Binding,
        intent: &ExecIntent,
        on_output: F,
    ) -> Result<ExecResult>
    where
        F: FnMut(ExecChunk) -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
        self.execute_inner(binding, intent, on_output, None).await
    }
    pub async fn execute_with_input<F, Fut>(
        &self,
        binding: &Binding,
        intent: &ExecIntent,
        input: ExecInput<'_>,
        on_output: F,
    ) -> Result<ExecResult>
    where
        F: FnMut(ExecChunk) -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
        if !intent.stdin.is_empty() {
            return Err(RuntimeFailure::InvalidExecution);
        }
        self.execute_inner(binding, intent, on_output, Some(input))
            .await
    }
    async fn execute_inner<F, Fut>(
        &self,
        binding: &Binding,
        intent: &ExecIntent,
        mut on_output: F,
        input: Option<ExecInput<'_>>,
    ) -> Result<ExecResult>
    where
        F: FnMut(ExecChunk) -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
        let current = self.get(binding).await?.ok_or(RuntimeFailure::NotFound)?;
        if current.phase != Phase::Ready || current.main_process_instance_id.is_empty() {
            return Err(RuntimeFailure::InvalidState);
        }
        tokio::time::timeout(
            Duration::from_secs(intent.timeout_seconds as u64 + 20),
            async {
                let start = api::ExecSandboxRequest {
                    sandbox_id: current.sandbox_id.clone(),
                    command: intent.command.clone(),
                    workdir: intent.workdir.clone(),
                    timeout_seconds: intent.timeout_seconds,
                    stdin: intent.stdin.clone(),
                    tty: false,
                    ..Default::default()
                };
                let (tx, rx) = mpsc::channel(1);
                let streaming = input.is_some();
                let mut stream = if streaming {
                    tx.send(api::ExecSandboxInput {
                        payload: Some(api::exec_sandbox_input::Payload::Start(start)),
                    })
                    .await
                    .map_err(|_| RuntimeFailure::ExecutionUnknown)?;
                    let requests = stream::unfold(rx, |mut rx| async move {
                        rx.recv().await.map(|item| (item, rx))
                    });
                    self.client
                        .clone()
                        .exec_sandbox_interactive(request(
                            requests,
                            intent.timeout_seconds as u64 + 15,
                        ))
                        .await
                } else {
                    self.client
                        .clone()
                        .exec_sandbox(request(start, intent.timeout_seconds as u64 + 15))
                        .await
                }
                .map_err(|_| RuntimeFailure::ExecutionUnknown)?
                .into_inner();
                // This producer is a borrowed future, never a detached task. Dropping
                // execute cancels its reads and releases the borrow before returning.
                // Keep tx alive until native exit: request EOF closes SSH in this pin.
                let producer = async {
                    if let Some(input) = input {
                        produce(input, &tx).await?;
                    }
                    Ok::<_, RuntimeFailure>(())
                };
                tokio::pin!(producer);
                let mut delivered = !streaming;
                let mut events = Events {
                    exit_code: None,
                    stdout: 0,
                    stderr: 0,
                    count: 0,
                    maximum: intent.maximum_output_bytes,
                };
                loop {
                    let next = tokio::select! {
                        result=&mut producer, if !delivered=>{ result?; delivered=true; continue; }
                        event=stream.message()=>event.map_err(|_|RuntimeFailure::ExecutionUnknown)?,
                    };
                    let Some(next) = next else {
                        break;
                    };
                    if let Some(chunk) = events.accept(next)? {
                        tokio::time::timeout(Duration::from_secs(10), on_output(chunk))
                            .await
                            .map_err(|_| RuntimeFailure::ExecutionUnknown)?
                            .map_err(|_| RuntimeFailure::ExecutionUnknown)?;
                    }
                }
                let result = events.result()?;
                if !delivered {
                    tokio::time::timeout(Duration::from_secs(1), &mut producer)
                        .await
                        .map_err(|_| RuntimeFailure::ExecutionUnknown)??;
                }
                let final_state = self
                    .get(binding)
                    .await
                    .map_err(|_| RuntimeFailure::ExecutionUnknown)?
                    .ok_or(RuntimeFailure::ExecutionUnknown)?;
                if !current.same_process(&final_state) {
                    return Err(RuntimeFailure::ExecutionUnknown);
                }
                Ok(result)
            },
        )
        .await
        .map_err(|_| RuntimeFailure::ExecutionUnknown)?
    }
}
async fn produce(input: ExecInput<'_>, tx: &mpsc::Sender<api::ExecSandboxInput>) -> Result<()> {
    let mut left = input.byte_count;
    while left > 0 {
        let mut data = vec![0; MAX_CHUNK_BYTES.min(left as usize)];
        let size = input
            .source
            .read(&mut data)
            .await
            .map_err(|_| RuntimeFailure::ExecutionUnknown)?;
        if size == 0 {
            return Err(RuntimeFailure::ExecutionUnknown);
        }
        data.truncate(size);
        left -= size as u64;
        tx.send(api::ExecSandboxInput {
            payload: Some(api::exec_sandbox_input::Payload::Stdin(data)),
        })
        .await
        .map_err(|_| RuntimeFailure::ExecutionUnknown)?;
    }
    if input
        .source
        .read(&mut [0])
        .await
        .map_err(|_| RuntimeFailure::ExecutionUnknown)?
        != 0
    {
        return Err(RuntimeFailure::ExecutionUnknown);
    }
    Ok(())
}
