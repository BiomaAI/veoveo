//! Verified private file transport. Artifact publication and authority are caller-owned.
use std::{
    future::Future,
    io::Cursor,
    sync::{Arc, Mutex},
};

use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt};
use veoveo_computer_execution::{FileFailure, FileReceipt, FileRequest, FileTransferBounds};

use crate::{
    Binding, ExecInput, ExecIntent, Observation, OpenShellRuntime, OutputStream, Phase, Result,
    RuntimeFailure,
};

pub(crate) const MAX_RESULT_BYTES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileTransferResult {
    Completed(FileReceipt),
    Rejected(FileFailure),
}

#[derive(Default)]
struct Output {
    result: Vec<u8>,
    bytes: u64,
    digest: Sha256,
}

impl OpenShellRuntime {
    /// The caller must hold current authority and a durable execution fence, and
    /// use a template qualified with the files/v1 helper. Export chunks are
    /// provisional until Completed verifies their exact byte count and digest.
    pub async fn transfer_file<F, Fut>(
        &self,
        binding: &Binding,
        expected: &Observation,
        request: &FileRequest,
        body: Option<&mut (dyn AsyncRead + Unpin + Send)>,
        timeout_seconds: u32,
        mut on_bytes: F,
    ) -> Result<FileTransferResult>
    where
        F: FnMut(Vec<u8>) -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
        if expected.phase != Phase::Ready
            || expected.exit_code.is_some()
            || !crate::models::identifier(&expected.sandbox_id)
            || !crate::models::identifier(&expected.main_process_instance_id)
        {
            return Err(RuntimeFailure::BindingMismatch);
        }
        let bounds = request.bounds();
        let (input_bytes, output_bytes) = match bounds {
            FileTransferBounds::Import { bytes, .. } => {
                if bytes != 0 && body.is_none() {
                    return Err(RuntimeFailure::InvalidExecution);
                }
                (bytes, 0)
            }
            FileTransferBounds::Export { maximum_bytes } => {
                if body.is_some() {
                    return Err(RuntimeFailure::InvalidExecution);
                }
                (0, maximum_bytes)
            }
        };
        let header = request
            .encode()
            .map_err(|_| RuntimeFailure::InvalidExecution)?;
        let total = header.len() as u64 + input_bytes;
        let mut empty = tokio::io::empty();
        let mut input = Cursor::new(header).chain(body.unwrap_or(&mut empty));
        let intent = ExecIntent::file_request(timeout_seconds, output_bytes as usize)?;
        let output = Arc::new(Mutex::new(Output::default()));
        let sink = output.clone();
        let result = self
            .execute_inner(
                binding,
                &intent,
                move |chunk| {
                    let next = (|| -> Result<Option<Fut>> {
                        let mut sink = sink.lock().map_err(|_| RuntimeFailure::ExecutionUnknown)?;
                        match chunk.stream {
                            OutputStream::Stdout => {
                                sink.bytes = sink
                                    .bytes
                                    .checked_add(chunk.data.len() as u64)
                                    .ok_or(RuntimeFailure::ExecutionUnknown)?;
                                if sink.bytes > output_bytes {
                                    return Err(RuntimeFailure::ExecutionUnknown);
                                }
                                sink.digest.update(&chunk.data);
                                Ok(Some(on_bytes(chunk.data)))
                            }
                            OutputStream::Stderr => {
                                if sink.result.len() + chunk.data.len() > MAX_RESULT_BYTES {
                                    return Err(RuntimeFailure::ExecutionUnknown);
                                }
                                sink.result.extend_from_slice(&chunk.data);
                                Ok(None)
                            }
                        }
                    })();
                    async move {
                        if let Some(next) = next? {
                            next.await?;
                        }
                        Ok(())
                    }
                },
                Some(ExecInput::new(&mut input, total)?),
                Some(expected),
            )
            .await?;
        let output = output
            .lock()
            .map_err(|_| RuntimeFailure::ExecutionUnknown)?;
        verify(bounds, result.exit_code, &output)
    }
}

fn verify(bounds: FileTransferBounds, exit: i32, output: &Output) -> Result<FileTransferResult> {
    let result: std::result::Result<FileReceipt, FileFailure> =
        serde_json::from_slice(&output.result).map_err(|_| RuntimeFailure::ExecutionUnknown)?;
    match (exit, result) {
        (0, Ok(receipt)) => {
            let valid = match bounds {
                FileTransferBounds::Import { bytes, sha256 } => {
                    receipt.bytes == bytes && receipt.sha256 == sha256 && output.bytes == 0
                }
                FileTransferBounds::Export { maximum_bytes } => {
                    receipt.bytes <= maximum_bytes
                        && receipt.bytes == output.bytes
                        && receipt.sha256 == <[u8; 32]>::from(output.digest.clone().finalize())
                }
            };
            if !valid {
                return Err(RuntimeFailure::ExecutionUnknown);
            }
            Ok(FileTransferResult::Completed(receipt))
        }
        (125, Err(failure)) if failure != FileFailure::CommitUnknown => {
            Ok(FileTransferResult::Rejected(failure))
        }
        _ => Err(RuntimeFailure::ExecutionUnknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_requires_matching_native_exit_count_and_digest() {
        let mut output = Output::default();
        output.digest.update(b"file");
        output.bytes = 4;
        let receipt = FileReceipt {
            bytes: 4,
            sha256: Sha256::digest(b"file").into(),
        };
        output.result = serde_json::to_vec(&Ok::<_, FileFailure>(receipt.clone())).unwrap();
        let bounds = FileTransferBounds::Export { maximum_bytes: 4 };
        assert_eq!(
            verify(bounds, 0, &output).unwrap(),
            FileTransferResult::Completed(receipt)
        );
        assert_eq!(
            verify(bounds, 125, &output),
            Err(RuntimeFailure::ExecutionUnknown)
        );
        output.bytes = 3;
        assert_eq!(
            verify(bounds, 0, &output),
            Err(RuntimeFailure::ExecutionUnknown)
        );
        output.bytes = 4;
        output.digest.update(b"tamper");
        assert_eq!(
            verify(bounds, 0, &output),
            Err(RuntimeFailure::ExecutionUnknown)
        );
        output.result =
            serde_json::to_vec(&Err::<FileReceipt, _>(FileFailure::CommitUnknown)).unwrap();
        assert_eq!(
            verify(bounds, 125, &output),
            Err(RuntimeFailure::ExecutionUnknown)
        );
        output.result = serde_json::to_vec(&Err::<FileReceipt, _>(FileFailure::Integrity)).unwrap();
        assert_eq!(
            verify(bounds, 125, &output).unwrap(),
            FileTransferResult::Rejected(FileFailure::Integrity)
        );
    }
}
