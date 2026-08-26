use std::{io, path::PathBuf, sync::Arc};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};
use tokio_util::sync::CancellationToken;

use crate::{
    domain::{EXECUTOR_PROTOCOL_VERSION, RunId},
    executor::{ExecutorOperation, ExecutorRequest, ExecutorResponse},
};

pub const DEFAULT_MAX_EXECUTOR_FRAME_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ExecutorClientError {
    #[error("cuOpt executor I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("cuOpt executor request encoding failed: {0}")]
    Encode(#[source] serde_json::Error),
    #[error("cuOpt executor response decoding failed: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("cuOpt executor frame is {actual} bytes and exceeds the {maximum}-byte limit")]
    FrameTooLarge { actual: u64, maximum: u64 },
    #[error("cuOpt executor returned protocol {actual}; expected {expected}")]
    ProtocolMismatch {
        actual: String,
        expected: &'static str,
    },
    #[error("cuOpt executor response run id {actual} does not match request run id {expected}")]
    RunMismatch { actual: RunId, expected: RunId },
    #[error("cuOpt execution was cancelled")]
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct ExecutorClient {
    socket_path: Arc<PathBuf>,
    max_frame_bytes: u64,
}

impl ExecutorClient {
    pub fn new(socket_path: impl Into<PathBuf>, max_frame_bytes: u64) -> Self {
        Self {
            socket_path: Arc::new(socket_path.into()),
            max_frame_bytes,
        }
    }

    pub fn with_default_limit(socket_path: impl Into<PathBuf>) -> Self {
        Self::new(socket_path, DEFAULT_MAX_EXECUTOR_FRAME_BYTES)
    }

    pub async fn health(&self) -> Result<ExecutorResponse, ExecutorClientError> {
        self.exchange(&ExecutorRequest::control(
            RunId::new(),
            ExecutorOperation::Health,
        ))
        .await
    }

    pub async fn execute(
        &self,
        request: &ExecutorRequest,
        cancellation: CancellationToken,
    ) -> Result<ExecutorResponse, ExecutorClientError> {
        tokio::select! {
            result = self.exchange(request) => result,
            () = cancellation.cancelled() => {
                let cancellation_request = ExecutorRequest::control(
                    RunId::new(),
                    ExecutorOperation::Cancel {
                        target_run_id: request.run_id.clone(),
                    },
                );
                let _ = self.exchange(&cancellation_request).await;
                Err(ExecutorClientError::Cancelled)
            }
        }
    }

    async fn exchange(
        &self,
        request: &ExecutorRequest,
    ) -> Result<ExecutorResponse, ExecutorClientError> {
        let request_bytes = serde_json::to_vec(request).map_err(ExecutorClientError::Encode)?;
        self.ensure_frame_size(request_bytes.len() as u64)?;

        let mut stream = UnixStream::connect(self.socket_path.as_ref()).await?;
        stream.write_u64(request_bytes.len() as u64).await?;
        stream.write_all(&request_bytes).await?;
        stream.flush().await?;

        let response_len = stream.read_u64().await?;
        self.ensure_frame_size(response_len)?;
        let response_len =
            usize::try_from(response_len).map_err(|_| ExecutorClientError::FrameTooLarge {
                actual: response_len,
                maximum: self.max_frame_bytes,
            })?;
        let mut response_bytes = vec![0_u8; response_len];
        stream.read_exact(&mut response_bytes).await?;
        let response: ExecutorResponse =
            serde_json::from_slice(&response_bytes).map_err(ExecutorClientError::Decode)?;
        if response.protocol != EXECUTOR_PROTOCOL_VERSION {
            return Err(ExecutorClientError::ProtocolMismatch {
                actual: response.protocol,
                expected: EXECUTOR_PROTOCOL_VERSION,
            });
        }
        if response.run_id != request.run_id {
            return Err(ExecutorClientError::RunMismatch {
                actual: response.run_id,
                expected: request.run_id.clone(),
            });
        }
        Ok(response)
    }

    fn ensure_frame_size(&self, actual: u64) -> Result<(), ExecutorClientError> {
        if actual > self.max_frame_bytes {
            Err(ExecutorClientError::FrameTooLarge {
                actual,
                maximum: self.max_frame_bytes,
            })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use tokio::net::UnixListener;

    use crate::executor::{ExecutorHealth, ExecutorResult};

    use super::*;

    #[tokio::test]
    async fn client_round_trips_a_length_prefixed_health_request() {
        let temp = TempDir::new().unwrap();
        let socket = temp.path().join("executor.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let length = stream.read_u64().await.unwrap();
            let mut bytes = vec![0_u8; length as usize];
            stream.read_exact(&mut bytes).await.unwrap();
            let request: ExecutorRequest = serde_json::from_slice(&bytes).unwrap();
            let response = ExecutorResponse {
                protocol: EXECUTOR_PROTOCOL_VERSION.to_owned(),
                run_id: request.run_id,
                result: ExecutorResult::Health {
                    health: ExecutorHealth {
                        ready: true,
                        cuopt_version: "26.08".to_owned(),
                        cuda_runtime_version: "13.2".to_owned(),
                        gpu_name: "test-gpu".to_owned(),
                        gpu_uuid: "GPU-test".to_owned(),
                        compute_capability: "9.0".to_owned(),
                    },
                },
            };
            let bytes = serde_json::to_vec(&response).unwrap();
            stream.write_u64(bytes.len() as u64).await.unwrap();
            stream.write_all(&bytes).await.unwrap();
        });

        let response = ExecutorClient::with_default_limit(socket)
            .health()
            .await
            .unwrap();
        assert!(matches!(
            response.result,
            ExecutorResult::Health {
                health: ExecutorHealth { ready: true, .. }
            }
        ));
        server.await.unwrap();
    }
}
