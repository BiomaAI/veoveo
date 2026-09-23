//! Private Unix-socket inference protocol. Credentials never enter this boundary.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{
        UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
};
use veoveo_speech_contract::transcript::Transcript;

pub const PROTOCOL: &str = "veoveo.speech-worker/v1";
pub const MAX_FRAME_BYTES: usize = 192_000;
pub const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerRequest {
    Probe,
    File {
        path: PathBuf,
        max_duration_seconds: u32,
    },
    Live {
        sample_rate: u32,
        max_duration_seconds: u32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkerEvent {
    Accepted,
    Ready {
        protocol: String,
        device: String,
        model: String,
        revision: String,
    },
    Transcript {
        complete: bool,
        transcript: Transcript,
    },
    Error {
        code: WorkerError,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerError {
    InvalidInput,
    Capacity,
    InferenceFailed,
    TimedOut,
}

pub struct WorkerConnection {
    reader: WorkerEvents,
    writer: PcmWriter,
}

pub struct WorkerEvents {
    reader: BufReader<OwnedReadHalf>,
    pending: Vec<u8>,
}
pub struct PcmWriter(OwnedWriteHalf);

impl WorkerConnection {
    pub async fn connect(socket: &Path, request: &WorkerRequest) -> Result<Self> {
        let connection = tokio::time::timeout(Duration::from_secs(5), UnixStream::connect(socket))
            .await
            .context("speech worker connection timed out")??;
        let (reader, mut writer) = connection.into_split();
        let mut bytes = serde_json::to_vec(request)?;
        ensure!(bytes.len() < 16_384, "speech worker request exceeds limit");
        bytes.push(b'\n');
        writer.write_all(&bytes).await?;
        Ok(Self {
            reader: WorkerEvents {
                reader: BufReader::new(reader),
                pending: Vec::new(),
            },
            writer: PcmWriter(writer),
        })
    }

    pub fn split(self) -> (WorkerEvents, PcmWriter) {
        (self.reader, self.writer)
    }

    /// Empty input flushes the final result. Dropping the connection cancels it.
    pub async fn pcm(&mut self, bytes: &[u8]) -> Result<()> {
        self.writer.pcm(bytes).await
    }

    pub async fn event(&mut self) -> Result<WorkerEvent> {
        self.reader.event().await
    }
}

impl PcmWriter {
    pub async fn pcm(&mut self, bytes: &[u8]) -> Result<()> {
        ensure!(
            bytes.len() <= MAX_FRAME_BYTES && bytes.len().is_multiple_of(4),
            "invalid PCM frame size"
        );
        self.0.write_u32(u32::try_from(bytes.len())?).await?;
        self.0.write_all(bytes).await?;
        Ok(())
    }
}

impl WorkerEvents {
    pub async fn event(&mut self) -> Result<WorkerEvent> {
        let length = (&mut self.reader)
            .take((MAX_RESPONSE_BYTES + 1 - self.pending.len()) as u64)
            .read_until(b'\n', &mut self.pending)
            .await?;
        ensure!(
            length > 0
                && self.pending.len() <= MAX_RESPONSE_BYTES
                && self.pending.last() == Some(&b'\n'),
            "speech worker returned a missing or oversized response"
        );
        let bytes = std::mem::take(&mut self.pending);
        serde_json::from_slice(&bytes).context("invalid speech worker response")
    }
}
