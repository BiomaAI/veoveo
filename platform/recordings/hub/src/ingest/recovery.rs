//! Preserve unaccepted terminal-stream journals without reopening the stream.
use std::{
    fs::{self, File},
    io::Write,
    os::unix::fs::DirBuilderExt,
    path::Path,
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_platform_store::{
    RecordingIngestStreamId, RecordingIngestStreamRecord, RecordingIngestStreamState, TenantId,
};
use veoveo_recording_protocol::v1::RecordingBatch;

pub(super) const QUARANTINE_DIRECTORY: &str = ".quarantine";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Schema {
    #[serde(rename = "veoveo.io/recording-journal-quarantine/v1")]
    V1,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Receipt {
    schema: Schema,
    tenant_id: uuid::Uuid,
    stream_id: uuid::Uuid,
    state: RecordingIngestStreamState,
    next_sequence: u64,
    sequence: u64,
    journal_sha256: String,
    journal_bytes: u64,
}

fn is_unaccepted_terminal(state: &RecordingIngestStreamState, sequence: u64, next: u64) -> bool {
    *state != RecordingIngestStreamState::Open && sequence >= next
}

pub(super) fn quarantine_terminal_batch(
    root: &Path,
    original: &Path,
    stream: &RecordingIngestStreamRecord,
    batch: &RecordingBatch,
    bytes: &[u8],
) -> Result<bool> {
    let next = u64::try_from(stream.next_sequence).context("negative ingest sequence")?;
    if !is_unaccepted_terminal(&stream.state, batch.sequence, next) {
        return Ok(false);
    }
    let receipt = Receipt {
        schema: Schema::V1,
        tenant_id: super::typed_record_uuid::<TenantId>(&stream.tenant, TenantId::TABLE)?.as_uuid(),
        stream_id: super::typed_record_uuid::<RecordingIngestStreamId>(
            &stream.id,
            RecordingIngestStreamId::TABLE,
        )?
        .as_uuid(),
        state: stream.state,
        next_sequence: next,
        sequence: batch.sequence,
        journal_sha256: hex::encode(Sha256::digest(bytes)),
        journal_bytes: bytes.len() as u64,
    };
    preserve(root, original, &receipt, bytes)?;
    tracing::warn!(stream_id = %receipt.stream_id, sequence = receipt.sequence, next_sequence = next,
        "Unaccepted terminal recording journal preserved in quarantine; operator recovery required");
    Ok(true)
}

fn sync(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn directory(path: &Path) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => sync(
            path.parent()
                .context("quarantine directory has no parent")?,
        )?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(
                fs::symlink_metadata(path)?.is_dir(),
                "quarantine path is not a directory"
            );
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn preserve(root: &Path, original: &Path, receipt: &Receipt, bytes: &[u8]) -> Result<()> {
    let quarantine = root.join(QUARANTINE_DIRECTORY);
    directory(&quarantine)?;
    let tenant = quarantine.join(receipt.tenant_id.to_string());
    directory(&tenant)?;
    let stream = tenant.join(receipt.stream_id.to_string());
    directory(&stream)?;
    let journal = stream.join(format!("{:020}.pb", receipt.sequence));
    let record = journal.with_extension("json");
    // A same-filesystem hard link preserves the exact original bytes without a
    // second copy. An interrupted attempt can only resume with identical data.
    match fs::hard_link(original, &journal) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(
                fs::symlink_metadata(&journal)?.is_file(),
                "quarantine journal is not a file"
            );
            ensure!(
                fs::read(&journal)? == bytes,
                "quarantine journal conflicts with retained bytes"
            );
        }
        Err(error) => return Err(error.into()),
    }
    sync(&journal)?;
    let mut temporary = tempfile::NamedTempFile::new_in(&stream)?;
    temporary.write_all(&serde_json::to_vec(receipt)?)?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(&record) {
        Ok(_) => {}
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(
                fs::symlink_metadata(&record)?.is_file(),
                "quarantine receipt is not a file"
            );
            let existing: Receipt = serde_json::from_slice(&fs::read(&record)?)?;
            ensure!(
                existing == *receipt,
                "quarantine receipt conflicts with terminal stream"
            );
        }
        Err(error) => return Err(error.into()),
    }
    sync(&stream)?;
    // Publication and its receipt are durable before the replay candidate is
    // removed. No stream state or materialization checkpoint is changed here.
    fs::remove_file(original)?;
    sync(original.parent().context("journal has no parent")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(bytes: &[u8]) -> Receipt {
        Receipt {
            schema: Schema::V1,
            tenant_id: uuid::Uuid::from_u128(1),
            stream_id: uuid::Uuid::from_u128(2),
            state: RecordingIngestStreamState::Finished,
            next_sequence: 7,
            sequence: 7,
            journal_sha256: hex::encode(Sha256::digest(bytes)),
            journal_bytes: bytes.len() as u64,
        }
    }
    #[test]
    fn accepted_duplicates_and_open_streams_still_replay() {
        for state in [
            RecordingIngestStreamState::Finished,
            RecordingIngestStreamState::Failed,
        ] {
            assert!(!is_unaccepted_terminal(&state, 6, 7));
            assert!(is_unaccepted_terminal(&state, 7, 7));
            assert!(is_unaccepted_terminal(&state, 8, 7));
        }
        assert!(!is_unaccepted_terminal(
            &RecordingIngestStreamState::Open,
            7,
            7
        ));
    }
    #[test]
    fn bytes_and_receipt_survive_repeated_recovery_without_overwrite() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = root.path().join("candidate.pb");
        let bytes = b"retained unaccepted journal";
        let receipt = receipt(bytes);
        fs::write(&source, bytes)?;
        preserve(root.path(), &source, &receipt, bytes)?;
        assert!(!source.exists());
        let target = root
            .path()
            .join(QUARANTINE_DIRECTORY)
            .join(receipt.tenant_id.to_string())
            .join(receipt.stream_id.to_string())
            .join("00000000000000000007.pb");
        assert_eq!(fs::read(&target)?, bytes);
        let saved: Receipt = serde_json::from_slice(&fs::read(target.with_extension("json"))?)?;
        assert_eq!(saved, receipt);
        // Simulate interruption after publishing both files, before unlinking.
        fs::hard_link(&target, &source)?;
        preserve(root.path(), &source, &receipt, bytes)?;
        fs::write(&source, b"conflicting journal")?;
        assert!(preserve(root.path(), &source, &receipt, b"conflicting journal").is_err());
        assert!(source.exists());
        assert_eq!(fs::read(target)?, bytes);
        Ok(())
    }
    #[test]
    fn receipt_conflict_and_symlink_keep_original() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = root.path().join("candidate.pb");
        let bytes = b"retained journal";
        let mut receipt = receipt(bytes);
        fs::write(&source, bytes)?;
        preserve(root.path(), &source, &receipt, bytes)?;
        fs::write(&source, bytes)?;
        receipt.next_sequence = 6;
        assert!(preserve(root.path(), &source, &receipt, bytes).is_err());
        assert!(source.exists());
        let other = tempfile::tempdir()?;
        std::os::unix::fs::symlink(
            root.path().join(QUARANTINE_DIRECTORY),
            other.path().join(QUARANTINE_DIRECTORY),
        )?;
        assert!(preserve(other.path(), &source, &receipt, bytes).is_err());
        assert!(source.exists());
        Ok(())
    }
}
