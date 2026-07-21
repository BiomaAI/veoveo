use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_recording_protocol::v1::RecordingBatch;

#[derive(Debug, thiserror::Error)]
#[error("durable recording queue is full")]
pub struct QueueFull;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueStream {
    pub key: String,
    pub source_stream_id: String,
    pub application_id: String,
    pub recording_id: String,
    pub remote_stream_id: Option<String>,
    pub remote_first_local_sequence: u64,
    pub next_enqueue_sequence: u64,
    pub next_upload_sequence: u64,
    pub finish_requested: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueuedBatch {
    pub local_sequence: u64,
    pub batch: RecordingBatch,
}

#[derive(Debug)]
pub struct DurableQueue {
    root: PathBuf,
    maximum_bytes: u64,
    queued_bytes: u64,
}

impl DurableQueue {
    pub fn open(root: PathBuf, maximum_bytes: u64) -> Result<Self> {
        ensure!(root.is_absolute(), "queue root must be absolute");
        ensure!(maximum_bytes > 0, "queue byte limit must be positive");
        std::fs::create_dir_all(&root)
            .with_context(|| format!("creating durable queue {}", root.display()))?;
        let root = root.canonicalize()?;
        let mut queue = Self {
            root,
            maximum_bytes,
            queued_bytes: 0,
        };
        queue.reconcile()?;
        Ok(queue)
    }

    pub fn enqueue(
        &mut self,
        application_id: &str,
        recording_id: &str,
        batch: &RecordingBatch,
    ) -> Result<(QueueStream, u64)> {
        validate_identity(application_id, recording_id)?;
        let added_bytes = u64::try_from(batch.encoded_len())?;
        if self.queued_bytes.saturating_add(added_bytes) > self.maximum_bytes {
            return Err(QueueFull.into());
        }
        let key = stream_key(application_id, recording_id);
        let directory = self.root.join(&key);
        std::fs::create_dir_all(&directory)?;
        sync_directory(&self.root)?;
        let mut stream = self.load_or_create_stream(&key, application_id, recording_id)?;
        let sequence = stream.next_enqueue_sequence;
        let mut batch = batch.clone();
        batch.sequence = sequence;
        let path = batch_path(&directory, sequence);
        let created = if path.exists() {
            let existing = RecordingBatch::decode(std::fs::read(&path)?.as_slice())?;
            ensure!(
                existing == batch,
                "queued batch sequence has conflicting content"
            );
            false
        } else {
            atomic_write(&path, &batch.encode_to_vec())?;
            true
        };
        stream.next_enqueue_sequence =
            sequence.checked_add(1).context("batch sequence overflow")?;
        self.write_stream(&stream)?;
        if created {
            self.queued_bytes = self.queued_bytes.saturating_add(added_bytes);
        }
        Ok((stream, sequence))
    }

    pub fn streams(&self) -> Result<Vec<QueueStream>> {
        let mut streams = Vec::<QueueStream>::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path().join("stream.json");
            if path.exists() {
                streams.push(read_json(&path)?);
            }
        }
        streams.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(streams)
    }

    pub fn next_batch(&self, stream: &QueueStream) -> Result<Option<QueuedBatch>> {
        validate_key(&stream.key)?;
        if stream.next_upload_sequence >= stream.next_enqueue_sequence {
            return Ok(None);
        }
        let local_sequence = stream.next_upload_sequence;
        let path = batch_path(&self.root.join(&stream.key), local_sequence);
        let bytes = std::fs::read(&path)
            .with_context(|| format!("reading queued batch {}", path.display()))?;
        let batch = RecordingBatch::decode(bytes.as_slice())
            .with_context(|| format!("decoding queued batch {}", path.display()))?;
        Ok(Some(QueuedBatch {
            local_sequence,
            batch,
        }))
    }

    pub fn has_batches(&self, stream: &QueueStream) -> Result<bool> {
        validate_key(&stream.key)?;
        Ok(stream.next_upload_sequence < stream.next_enqueue_sequence)
    }

    pub fn mark_opened(
        &mut self,
        stream: &QueueStream,
        remote_stream_id: &str,
    ) -> Result<QueueStream> {
        validate_remote_stream_id(remote_stream_id)?;
        let mut current = self.read_stream(&stream.key)?;
        ensure!(
            current
                .remote_stream_id
                .as_deref()
                .is_none_or(|id| id == remote_stream_id),
            "gateway returned a different stream for the same source stream"
        );
        current.remote_stream_id = Some(remote_stream_id.to_owned());
        self.write_stream(&current)?;
        Ok(current)
    }

    pub fn rollover(&mut self, stream: &QueueStream) -> Result<QueueStream> {
        let mut current = self.read_stream(&stream.key)?;
        ensure!(
            current.next_upload_sequence < current.next_enqueue_sequence,
            "cannot roll over an empty queued stream"
        );
        current.source_stream_id = uuid::Uuid::now_v7().to_string();
        current.remote_stream_id = None;
        current.remote_first_local_sequence = current.next_upload_sequence;
        self.write_stream(&current)?;
        Ok(current)
    }

    pub fn acknowledge(&mut self, stream: &QueueStream, sequence: u64) -> Result<QueueStream> {
        let mut current = self.read_stream(&stream.key)?;
        ensure!(
            current.next_upload_sequence == sequence,
            "acknowledged batch is not the next queued batch"
        );
        let path = batch_path(&self.root.join(&stream.key), sequence);
        ensure!(path.exists(), "acknowledged batch is not queued");
        let byte_len = std::fs::metadata(&path)?.len();
        std::fs::remove_file(&path)?;
        sync_directory(path.parent().context("batch path has no parent")?)?;
        current.next_upload_sequence = sequence
            .checked_add(1)
            .context("upload sequence overflow")?;
        self.write_stream(&current)?;
        self.queued_bytes = self.queued_bytes.saturating_sub(byte_len);
        Ok(current)
    }

    pub fn request_finish_all(&mut self) -> Result<()> {
        for mut stream in self.streams()? {
            if !stream.finish_requested {
                stream.finish_requested = true;
                self.write_stream(&stream)?;
            }
        }
        Ok(())
    }

    pub fn complete(&mut self, stream: &QueueStream) -> Result<()> {
        ensure!(
            !self.has_batches(stream)?,
            "cannot complete a queued stream with batches"
        );
        let directory = self.root.join(&stream.key);
        std::fs::remove_file(directory.join("stream.json"))?;
        sync_directory(&directory)?;
        std::fs::remove_dir(&directory)?;
        sync_directory(&self.root)
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.streams()?.iter().all(|stream| {
            self.has_batches(stream)
                .is_ok_and(|has_batches| !has_batches)
        }))
    }

    fn reconcile(&mut self) -> Result<()> {
        let mut queued_bytes = 0_u64;
        for stream in self.streams()? {
            let inventory = self.batch_inventory(&stream)?;
            queued_bytes = queued_bytes.saturating_add(inventory.byte_len);
            let next_enqueue_sequence = inventory
                .last_sequence
                .map(|sequence| sequence.saturating_add(1))
                .unwrap_or(stream.next_enqueue_sequence)
                .max(stream.next_enqueue_sequence);
            let next_upload_sequence = inventory.first_sequence.unwrap_or(next_enqueue_sequence);
            if next_enqueue_sequence != stream.next_enqueue_sequence
                || next_upload_sequence != stream.next_upload_sequence
            {
                let mut repaired = stream;
                repaired.next_enqueue_sequence = next_enqueue_sequence;
                repaired.next_upload_sequence = next_upload_sequence;
                self.write_stream(&repaired)?;
            }
        }
        self.queued_bytes = queued_bytes;
        Ok(())
    }

    fn batch_inventory(&self, stream: &QueueStream) -> Result<BatchInventory> {
        validate_key(&stream.key)?;
        let mut inventory = BatchInventory::default();
        for entry in std::fs::read_dir(self.root.join(&stream.key))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("pb") {
                continue;
            }
            let sequence = path
                .file_stem()
                .and_then(|value| value.to_str())
                .context("queued batch filename is not UTF-8")?
                .parse::<u64>()
                .with_context(|| format!("invalid queued batch filename {}", path.display()))?;
            inventory.first_sequence = Some(
                inventory
                    .first_sequence
                    .map_or(sequence, |current| current.min(sequence)),
            );
            inventory.last_sequence = Some(
                inventory
                    .last_sequence
                    .map_or(sequence, |current| current.max(sequence)),
            );
            inventory.byte_len = inventory.byte_len.saturating_add(entry.metadata()?.len());
        }
        Ok(inventory)
    }

    fn load_or_create_stream(
        &self,
        key: &str,
        application_id: &str,
        recording_id: &str,
    ) -> Result<QueueStream> {
        let path = self.root.join(key).join("stream.json");
        if path.exists() {
            let stream: QueueStream = read_json(&path)?;
            ensure!(
                stream.application_id == application_id && stream.recording_id == recording_id,
                "queue key collides with a different Rerun identity"
            );
            return Ok(stream);
        }
        let stream = QueueStream {
            key: key.to_owned(),
            source_stream_id: uuid::Uuid::now_v7().to_string(),
            application_id: application_id.to_owned(),
            recording_id: recording_id.to_owned(),
            remote_stream_id: None,
            remote_first_local_sequence: 1,
            next_enqueue_sequence: 1,
            next_upload_sequence: 1,
            finish_requested: false,
        };
        self.write_stream(&stream)?;
        Ok(stream)
    }

    fn read_stream(&self, key: &str) -> Result<QueueStream> {
        validate_key(key)?;
        read_json(&self.root.join(key).join("stream.json"))
    }

    fn write_stream(&self, stream: &QueueStream) -> Result<()> {
        validate_key(&stream.key)?;
        let bytes = serde_json::to_vec(stream)?;
        atomic_write(&self.root.join(&stream.key).join("stream.json"), &bytes)
    }
}

#[derive(Debug, Default)]
struct BatchInventory {
    first_sequence: Option<u64>,
    last_sequence: Option<u64>,
    byte_len: u64,
}

fn stream_key(application_id: &str, recording_id: &str) -> String {
    hex::encode(Sha256::digest(
        format!("{application_id}\0{recording_id}").as_bytes(),
    ))
}

fn batch_path(directory: &Path, sequence: u64) -> PathBuf {
    directory.join(format!("{sequence:020}.pb"))
}

fn validate_key(key: &str) -> Result<()> {
    ensure!(
        key.len() == 64 && key.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "queue stream key is invalid"
    );
    Ok(())
}

fn validate_identity(application_id: &str, recording_id: &str) -> Result<()> {
    for (field, value) in [
        ("application_id", application_id),
        ("recording_id", recording_id),
    ] {
        ensure!(
            !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control),
            "{field} is empty or invalid"
        );
    }
    Ok(())
}

fn validate_remote_stream_id(value: &str) -> Result<()> {
    let id = uuid::Uuid::parse_str(value)?;
    ensure!(id.get_version_num() == 7, "remote stream ID is not UUIDv7");
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("durable path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::now_v7()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    sync_directory(parent)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    serde_json::from_slice(&std::fs::read(path)?)
        .with_context(|| format!("reading {}", path.display()))
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;
    use veoveo_recording_protocol::v1::RerunPayloadFormat;

    use super::*;

    fn batch() -> RecordingBatch {
        let payload = b"complete-rrd".to_vec();
        RecordingBatch {
            sequence: 0,
            payload_format: RerunPayloadFormat::Rrd0341.into(),
            sha256: Sha256::digest(&payload).to_vec(),
            encoded_rrd: payload,
            message_count: 1,
        }
    }

    #[test]
    fn durable_queue_reopens_and_removes_only_acknowledged_batches() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("queue");
        let mut queue = DurableQueue::open(root.clone(), 1_000_000).unwrap();
        let (stream, sequence) = queue.enqueue("camera", "run-a", &batch()).unwrap();
        assert_eq!(sequence, 1);
        let (_, second_sequence) = queue.enqueue("camera", "run-a", &batch()).unwrap();
        assert_eq!(second_sequence, 2);
        drop(queue);

        let mut queue = DurableQueue::open(root, 1_000_000).unwrap();
        let streams = queue.streams().unwrap();
        assert_eq!(streams.len(), 1);
        assert_eq!(
            queue
                .next_batch(&streams[0])
                .unwrap()
                .unwrap()
                .local_sequence,
            1
        );
        let stream = queue.acknowledge(&stream, 1).unwrap();
        assert_eq!(
            queue.next_batch(&stream).unwrap().unwrap().local_sequence,
            2
        );
        let stream = queue.acknowledge(&stream, 2).unwrap();
        assert!(!queue.has_batches(&stream).unwrap());
    }

    #[test]
    fn durable_queue_applies_disk_backpressure() {
        let temporary = TempDir::new().unwrap();
        let mut queue = DurableQueue::open(temporary.path().join("queue"), 1).unwrap();
        assert!(queue.enqueue("camera", "run-a", &batch()).is_err());
    }

    #[test]
    fn finish_intent_survives_restart_after_batches_are_acknowledged() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("queue");
        let mut queue = DurableQueue::open(root.clone(), 1_000_000).unwrap();
        let (stream, sequence) = queue.enqueue("camera", "run-a", &batch()).unwrap();
        queue.acknowledge(&stream, sequence).unwrap();
        queue.request_finish_all().unwrap();
        drop(queue);

        let queue = DurableQueue::open(root, 1_000_000).unwrap();
        let streams = queue.streams().unwrap();
        assert_eq!(streams.len(), 1);
        assert!(streams[0].finish_requested);
        assert!(!queue.has_batches(&streams[0]).unwrap());
    }

    #[test]
    fn acknowledged_bytes_restore_queue_capacity() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("queue");
        let encoded_len = u64::try_from(batch().encoded_len()).unwrap();
        let mut queue = DurableQueue::open(root, encoded_len).unwrap();
        let (stream, sequence) = queue.enqueue("camera", "run-a", &batch()).unwrap();
        assert!(queue.enqueue("camera", "run-a", &batch()).is_err());
        queue.acknowledge(&stream, sequence).unwrap();
        assert!(queue.enqueue("camera", "run-a", &batch()).is_ok());
    }

    #[test]
    fn rollover_starts_a_new_remote_generation_at_the_pending_batch() {
        let temporary = TempDir::new().unwrap();
        let mut queue = DurableQueue::open(temporary.path().join("queue"), 1_000_000).unwrap();
        let (_stream, first) = queue.enqueue("camera", "run-a", &batch()).unwrap();
        let (stream, second) = queue.enqueue("camera", "run-a", &batch()).unwrap();
        let original_source_stream_id = stream.source_stream_id.clone();
        let remote_stream_id = uuid::Uuid::now_v7().to_string();
        let stream = queue.mark_opened(&stream, &remote_stream_id).unwrap();
        let stream = queue.acknowledge(&stream, first).unwrap();
        let stream = queue.rollover(&stream).unwrap();

        assert_eq!(stream.remote_stream_id, None);
        assert_ne!(stream.source_stream_id, original_source_stream_id);
        assert_eq!(stream.remote_first_local_sequence, second);
        assert_eq!(
            queue.next_batch(&stream).unwrap().unwrap().local_sequence,
            second
        );
    }
}
