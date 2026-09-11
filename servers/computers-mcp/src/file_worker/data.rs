use super::*;
use futures::StreamExt;
use sha2::{Digest, Sha256};
use veoveo_computer_execution::FileFailure;
use veoveo_computers::{api::FileTransfer, files::FilePreparation, secrets::FileTransferAccess};
use veoveo_mcp_contract::{
    ArtifactId, ArtifactReadAuthority, ArtifactWriteIdempotencyKey, PutArtifactRequest,
    RedeemArtifactWriteCapabilityRequest,
};

pub(super) struct Buffer {
    bytes: Vec<u8>,
    maximum: u64,
}
impl Buffer {
    pub fn new(maximum: u64) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
        }
    }
    pub fn constrain(&mut self, maximum: u64) -> bool {
        self.maximum = self.maximum.min(maximum);
        self.bytes.len() as u64 <= self.maximum
    }
    pub fn append(&mut self, bytes: &[u8]) -> Result<()> {
        let total = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .ok_or(FileWorkerError::ArtifactUnavailable)?;
        if total as u64 > self.maximum {
            return Err(FileWorkerError::ArtifactUnavailable);
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub fn maximum(&self) -> u64 {
        self.maximum
    }
    pub fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
}

impl FileWorker {
    pub(super) async fn prepare_source(
        &self,
        preparation: &FilePreparation,
        capture: Arc<Mutex<Buffer>>,
    ) -> Result<Option<[u8; 32]>> {
        let FileTransfer::Import { artifact_id, .. } = preparation.payload.transfer() else {
            return Ok(None);
        };
        let FileTransferAccess::Import { capability } = &preparation.access else {
            return Err(FileWorkerError::Configuration);
        };
        let source = ArtifactId::parse(artifact_id.to_string())
            .map_err(|_| FileWorkerError::Configuration)?;
        let download = self
            .artifacts
            .download_with_authority(
                ArtifactReadAuthority::Task {
                    capability,
                    task_id: capability.task_id,
                },
                source,
            )
            .await
            .map_err(|_| FileWorkerError::ArtifactUnavailable)?;
        let actor = preparation.operation.actor();
        let metadata = &download.metadata;
        let floor = &preparation.retained_labels;
        // Imported data becomes retained Computer data. Its complete label floor
        // must already be covered by this Computer, even when the caller has more clearance.
        if metadata.artifact_id != source
            || metadata.artifact_uri != source.plane_uri()
            || metadata.compliance.tenant_id.as_ref() != Some(&actor.authority.tenant)
            || !metadata.compliance.data_labels.is_subset(floor)
            || metadata
                .compliance
                .classification
                .as_ref()
                .is_some_and(|label| !floor.contains(label))
            || metadata.byte_len
                > capture
                    .lock()
                    .map_err(|_| FileWorkerError::Configuration)?
                    .maximum()
        {
            return Err(FileWorkerError::ArtifactUnavailable);
        }
        let expected = metadata.byte_len;
        let mut observed = 0_u64;
        let mut hash = Sha256::new();
        let mut chunks = download.response.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk.map_err(|_| FileWorkerError::ArtifactUnavailable)?;
            observed = observed
                .checked_add(chunk.len() as u64)
                .ok_or(FileWorkerError::ArtifactUnavailable)?;
            if observed > expected {
                return Err(FileWorkerError::ArtifactUnavailable);
            }
            capture
                .lock()
                .map_err(|_| FileWorkerError::Configuration)?
                .append(&chunk)?;
            hash.update(&chunk);
        }
        if observed != expected {
            return Err(FileWorkerError::ArtifactUnavailable);
        }
        Ok(Some(hash.finalize().into()))
    }

    pub(super) async fn publish(
        &self,
        ticket: &FileExitTicket,
        bytes: Vec<u8>,
    ) -> Result<Option<uuid::Uuid>> {
        let receipt = match ticket.receipt() {
            Ok(receipt) => receipt,
            Err(_) => return Ok(None),
        };
        let (
            FileTransfer::Export {
                filename,
                media_type,
                ..
            },
            FileTransferAccess::Export { capability },
        ) = (ticket.transfer(), ticket.access())
        else {
            return match ticket.transfer() {
                FileTransfer::Import { artifact_id, .. } => Ok(Some(*artifact_id)),
                _ => Err(FileWorkerError::Configuration),
            };
        };
        if bytes.len() as u64 != receipt.bytes
            || <[u8; 32]>::from(Sha256::digest(&bytes)) != receipt.sha256
        {
            return Err(FileWorkerError::ArtifactUnavailable);
        }
        let operation = ticket.operation();
        let descriptor = serde_json::json!({"computer_id":operation.computer_id(),"transfer_id":operation.transfer_id(),"sha256":hex::encode(receipt.sha256)});
        let request = RedeemArtifactWriteCapabilityRequest {
            capability_id: capability.capability_id,
            task_id: capability.task_id.clone(),
            idempotency_key: ArtifactWriteIdempotencyKey::new("file")
                .map_err(|_| FileWorkerError::Configuration)?,
            artifact: PutArtifactRequest {
                filename: Some(filename.clone()),
                mime_type: Some(media_type.clone()),
                metadata: descriptor.clone(),
                ..Default::default()
            },
        };
        let metadata = self
            .artifacts
            .redeem_write_capability(&capability.secret, &request, bytes)
            .await
            .map_err(|_| FileWorkerError::ArtifactUnavailable)?;
        let actor = operation.actor();
        if metadata.byte_len != receipt.bytes
            || metadata.artifact_uri != metadata.artifact_id.plane_uri()
            || metadata.metadata != descriptor
            || metadata.filename.as_ref() != Some(filename)
            || metadata.mime_type.as_ref() != Some(media_type)
            || metadata.compliance.tenant_id.as_ref() != Some(&actor.authority.tenant)
            || metadata.compliance.work_context.as_ref() != Some(&actor.authority.work_context)
            || !operation
                .binding()
                .required_labels
                .is_subset(&metadata.compliance.data_labels)
        {
            return Err(FileWorkerError::ArtifactUnavailable);
        }
        Ok(Some(metadata.artifact_id.as_uuid()))
    }
}

pub(super) fn rejection_message(reason: FileFailure) -> &'static str {
    match reason {
        FileFailure::DestinationExists => {
            "A file already exists at the destination; choose a new path"
        }
        FileFailure::TooLarge => "The file exceeds this transfer's byte limit",
        FileFailure::PathUnavailable => "The retained file path is unavailable",
        FileFailure::UnsupportedFile => "Choose a regular file inside the retained home",
        FileFailure::FileChanged => {
            "The source file changed during transfer; retry when it is stable"
        }
        FileFailure::Integrity | FileFailure::InputIncomplete => {
            "The file did not pass transfer verification"
        }
        FileFailure::InvalidRequest => "The file transfer request was rejected",
        FileFailure::WrongIdentity | FileFailure::ConfinementUnavailable => {
            "The Computer file profile is unavailable"
        }
        FileFailure::Storage | FileFailure::OutputUnavailable => {
            "The file could not be transferred"
        }
        FileFailure::CommitUnknown => "The file outcome requires recovery",
    }
}
