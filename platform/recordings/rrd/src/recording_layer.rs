//! Canonical RRD recording-layer identity and byte validation.

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, ensure};
use re_log_encoding::rrd::{CrateVersion, Decoder, Encoder, EncodingOptions};
use re_log_types::{ApplicationId, LogMsg, StoreId};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalRecordingLayer {
    pub byte_len: u64,
    pub message_count: u64,
    pub sha256: String,
    pub schema_digest: String,
    pub rrd_version: String,
}

/// Rewrite one producer-authored recording RRD to the only catalog identity:
/// dataset UUID as application ID and recording UUID as recording ID.
///
/// The replacement is written beside the source, synced, validated, and
/// atomically renamed over it. A malformed or multi-store source is left
/// untouched.
pub fn normalize_recording_layer(
    path: &Path,
    dataset_id: Uuid,
    recording_id: Uuid,
) -> Result<CanonicalRecordingLayer> {
    ensure!(
        dataset_id.get_version_num() == 7,
        "dataset identity must be UUIDv7"
    );
    ensure!(
        recording_id.get_version_num() == 7,
        "recording identity must be UUIDv7"
    );
    if let Ok(existing) = inspect_canonical_recording_layer(path, dataset_id, recording_id) {
        return Ok(existing);
    }
    let source =
        File::open(path).with_context(|| format!("opening recording layer {}", path.display()))?;
    source
        .sync_all()
        .with_context(|| format!("syncing recording layer {}", path.display()))?;
    let partial = normalization_partial_path(path)?;
    match std::fs::remove_file(&partial) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("removing stale normalization partial"),
    }
    let output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&partial)
        .with_context(|| format!("creating recording layer partial {}", partial.display()))?;
    let canonical_id = canonical_store_id(dataset_id, recording_id)?;
    let result = normalize_messages(source, output, &canonical_id);
    let mut normalized = match result {
        Ok(normalized) => normalized,
        Err(error) => {
            let _ = std::fs::remove_file(&partial);
            return Err(error);
        }
    };
    let (byte_len, sha256) = hash_file(&partial)?;
    normalized.byte_len = byte_len;
    normalized.sha256 = sha256;
    std::fs::rename(&partial, path).with_context(|| {
        format!(
            "atomically installing normalized recording layer {}",
            path.display()
        )
    })?;
    sync_parent(path)?;
    let inspected = inspect_canonical_recording_layer(path, dataset_id, recording_id)?;
    ensure!(
        inspected == normalized,
        "installed recording layer differs from normalized bytes"
    );
    Ok(inspected)
}

pub fn inspect_canonical_recording_layer(
    path: &Path,
    dataset_id: Uuid,
    recording_id: Uuid,
) -> Result<CanonicalRecordingLayer> {
    let expected = canonical_store_id(dataset_id, recording_id)?;
    let file = File::open(path)
        .with_context(|| format!("opening canonical recording layer {}", path.display()))?;
    let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(file))
        .with_context(|| format!("decoding canonical recording layer {}", path.display()))?;
    let mut message_count = 0_u64;
    let mut schemas = Sha256::new();
    for message in decoder {
        let message = message.context("decoding canonical RRD message")?;
        ensure!(
            message.store_id() == &expected,
            "recording layer contains a noncanonical Store ID"
        );
        hash_schema(&message, &mut schemas)?;
        message_count = message_count
            .checked_add(1)
            .context("recording layer message count overflow")?;
    }
    ensure!(message_count > 0, "recording layer contains no messages");
    let (byte_len, sha256) = hash_file(path)?;
    Ok(CanonicalRecordingLayer {
        byte_len,
        message_count,
        sha256,
        schema_digest: hex::encode(schemas.finalize()),
        rrd_version: CrateVersion::LOCAL.to_string(),
    })
}

fn normalize_messages(
    source: File,
    output: File,
    canonical_id: &StoreId,
) -> Result<CanonicalRecordingLayer> {
    let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(source))
        .context("decoding producer recording layer")?;
    let mut encoder = Encoder::new_eager(
        CrateVersion::LOCAL,
        EncodingOptions::PROTOBUF_COMPRESSED,
        output,
    )
    .context("creating canonical recording-layer encoder")?;
    let mut source_ids = BTreeSet::new();
    let mut message_count = 0_u64;
    let mut schemas = Sha256::new();
    for message in decoder {
        let mut message = message.context("decoding producer RRD message")?;
        ensure!(
            message.store_id().is_recording(),
            "capture layer contains a non-recording store"
        );
        source_ids.insert(message.store_id().clone());
        ensure!(
            source_ids.len() == 1,
            "capture layer contains more than one Store ID"
        );
        message.set_store_id(canonical_id.clone());
        hash_schema(&message, &mut schemas)?;
        encoder
            .append(&message)
            .context("encoding canonical RRD message")?;
        message_count = message_count
            .checked_add(1)
            .context("recording layer message count overflow")?;
    }
    ensure!(message_count > 0, "capture layer contains no messages");
    encoder
        .finish()
        .context("finishing canonical recording layer")?;
    let output = encoder
        .into_inner()
        .context("recovering canonical recording layer file")?;
    output
        .sync_all()
        .context("syncing canonical recording layer")?;
    let path_len = output
        .metadata()
        .context("reading canonical recording layer metadata")?
        .len();
    drop(output);
    Ok(CanonicalRecordingLayer {
        byte_len: path_len,
        message_count,
        sha256: String::new(),
        schema_digest: hex::encode(schemas.finalize()),
        rrd_version: CrateVersion::LOCAL.to_string(),
    })
}

fn hash_schema(message: &LogMsg, digest: &mut Sha256) -> Result<()> {
    if let LogMsg::ArrowMsg(_, arrow) = message {
        let schema = arrow.batch.schema();
        for field in schema.fields() {
            digest.update(field.name().as_bytes());
            digest.update([u8::from(field.is_nullable())]);
            digest.update(format!("{:?}", field.data_type()).as_bytes());
            let mut metadata = field.metadata().iter().collect::<Vec<_>>();
            metadata.sort_unstable();
            for (key, value) in metadata {
                digest.update(key.as_bytes());
                digest.update([0]);
                digest.update(value.as_bytes());
                digest.update([0]);
            }
        }
        let mut metadata = schema.metadata().iter().collect::<Vec<_>>();
        metadata.sort_unstable();
        for (key, value) in metadata {
            digest.update(key.as_bytes());
            digest.update([0]);
            digest.update(value.as_bytes());
            digest.update([0]);
        }
    }
    Ok(())
}

fn canonical_store_id(dataset_id: Uuid, recording_id: Uuid) -> Result<StoreId> {
    let application_id = ApplicationId::try_new(dataset_id.to_string())
        .context("dataset UUID is not a valid Rerun application ID")?;
    Ok(StoreId::recording(application_id, recording_id.to_string()))
}

fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut file =
        File::open(path).with_context(|| format!("opening {} for hash", path.display()))?;
    let byte_len = file.metadata()?.len();
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok((byte_len, hex::encode(digest.finalize())))
}

fn normalization_partial_path(path: &Path) -> Result<PathBuf> {
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .context("recording layer filename is not UTF-8")?;
    Ok(path.with_file_name(format!(".{filename}.normalizing.partial")))
}

fn sync_parent(path: &Path) -> Result<()> {
    let parent = path.parent().context("recording layer has no parent")?;
    File::open(parent)
        .with_context(|| format!("opening recording layer directory {}", parent.display()))?
        .sync_all()
        .with_context(|| format!("syncing recording layer directory {}", parent.display()))
}

#[cfg(test)]
mod tests {
    use re_log_encoding::rrd::{Encoder, EncodingOptions};
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;

    use super::*;

    #[test]
    fn producer_identity_is_replaced_deterministically() {
        let (recording, storage) = RecordingStreamBuilder::new("producer-controlled")
            .recording_id("producer-run")
            .memory()
            .unwrap();
        recording
            .log("sensor/value", &Scalars::single(42.0))
            .unwrap();
        drop(recording);
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.rrd");
        let messages = storage.take();
        let mut encoder = Encoder::new_eager(
            CrateVersion::LOCAL,
            EncodingOptions::PROTOBUF_COMPRESSED,
            File::create(&first).unwrap(),
        )
        .unwrap();
        for message in &messages {
            encoder.append(message).unwrap();
        }
        encoder.finish().unwrap();
        let dataset_id = Uuid::now_v7();
        let recording_id = Uuid::now_v7();
        let first_result = normalize_recording_layer(&first, dataset_id, recording_id).unwrap();
        let normalized_bytes = std::fs::read(&first).unwrap();
        let second_result = normalize_recording_layer(&first, dataset_id, recording_id).unwrap();
        assert_eq!(first_result, second_result);
        assert_eq!(normalized_bytes, std::fs::read(&first).unwrap());
        assert!(!first_result.sha256.is_empty());
    }
}
