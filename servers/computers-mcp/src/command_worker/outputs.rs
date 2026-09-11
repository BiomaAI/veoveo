use super::*;
use serde::Serialize;
use veoveo_computers::api::ExecutionOutput;
use veoveo_computers_runtime::{ExecChunk, OutputStream, RuntimeFailure};
use veoveo_mcp_contract::{
    ArtifactWriteIdempotencyKey, PutArtifactRequest, RedeemArtifactWriteCapabilityRequest,
};

pub(super) struct CapturedOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    maximum: u32,
}
impl CapturedOutput {
    pub fn new(maximum: u32) -> Self {
        Self {
            stdout: vec![],
            stderr: vec![],
            maximum,
        }
    }
    pub fn constrain(&mut self, maximum: u32) -> bool {
        self.maximum = self.maximum.min(maximum);
        self.stdout.len() + self.stderr.len() <= self.maximum as usize
    }
    pub fn append(&mut self, chunk: ExecChunk) -> std::result::Result<(), RuntimeFailure> {
        if self.stdout.len() + self.stderr.len() + chunk.data.len() > self.maximum as usize {
            return Err(RuntimeFailure::ExecutionUnknown);
        }
        match chunk.stream {
            OutputStream::Stdout => self.stdout.extend(chunk.data),
            OutputStream::Stderr => self.stderr.extend(chunk.data),
        }
        Ok(())
    }
    pub fn take(&mut self) -> (Vec<u8>, Vec<u8>) {
        (
            std::mem::take(&mut self.stdout),
            std::mem::take(&mut self.stderr),
        )
    }
}

impl CommandWorker {
    pub(super) async fn publish(
        &self,
        ticket: &CommandExitTicket,
        (stdout, stderr): (Vec<u8>, Vec<u8>),
    ) -> Result<(ExecutionOutput, ExecutionOutput)> {
        // Both occurrences charge the same capability quota row. Serialize
        // them to avoid turning our own competing writes into a failed result.
        let stdout = self.publish_stream(ticket, "stdout", stdout).await?;
        let stderr = self.publish_stream(ticket, "stderr", stderr).await?;
        Ok((stdout, stderr))
    }
    async fn publish_stream(
        &self,
        ticket: &CommandExitTicket,
        stream: &'static str,
        bytes: Vec<u8>,
    ) -> Result<ExecutionOutput> {
        #[derive(Serialize)]
        struct Descriptor {
            computer_id: uuid::Uuid,
            execution_id: uuid::Uuid,
            stream: &'static str,
        }
        let operation = ticket.operation();
        let descriptor = serde_json::to_value(Descriptor {
            computer_id: operation.computer_id(),
            execution_id: operation.execution_id(),
            stream,
        })
        .map_err(|_| CommandWorkerError::Configuration)?;
        let capability = ticket.output_access().capability();
        let count = bytes.len() as u64;
        let (mime_type, extension) = if std::str::from_utf8(&bytes).is_ok() {
            ("text/plain; charset=utf-8", "txt")
        } else {
            ("application/octet-stream", "bin")
        };
        let request = RedeemArtifactWriteCapabilityRequest {
            capability_id: capability.capability_id,
            task_id: capability.task_id.clone(),
            idempotency_key: ArtifactWriteIdempotencyKey::new(stream)
                .map_err(|_| CommandWorkerError::Configuration)?,
            artifact: PutArtifactRequest {
                mime_type: Some(mime_type.into()),
                filename: Some(format!("{stream}.{extension}")),
                metadata: descriptor.clone(),
                ..Default::default()
            },
        };
        let metadata = self
            .artifacts
            .redeem_write_capability(&capability.secret, &request, bytes)
            .await
            .map_err(|error| {
                use veoveo_mcp_contract::ArtifactPlaneError;
                let class = match error {
                    ArtifactPlaneError::NotFound => "not_found",
                    ArtifactPlaneError::Denied(_) => "denied",
                    ArtifactPlaneError::Unauthenticated => "unauthenticated",
                    ArtifactPlaneError::InvalidRequest(_) => "invalid_request",
                    ArtifactPlaneError::Conflict(_) => "conflict",
                    ArtifactPlaneError::Transport(_) => "transport",
                };
                tracing::warn!(execution_id = %operation.execution_id(), stream, class, "Computer Artifact publication failed");
                CommandWorkerError::OutputUnavailable
            })?;
        let actor = operation.actor();
        if metadata.byte_len != count
            || metadata.artifact_uri != metadata.artifact_id.plane_uri()
            || metadata.metadata != descriptor
            || metadata.compliance.tenant_id.as_ref() != Some(&actor.authority.tenant)
            || metadata.compliance.work_context.as_ref() != Some(&actor.authority.work_context)
            || !operation
                .binding()
                .required_output_labels
                .is_subset(&metadata.compliance.data_labels)
        {
            tracing::warn!(execution_id = %operation.execution_id(), stream, "Computer Artifact receipt failed validation");
            return Err(CommandWorkerError::OutputUnavailable);
        }
        Ok(ExecutionOutput {
            artifact_id: metadata.artifact_id.as_uuid(),
            byte_count: count as u32,
        })
    }
}
