use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use prost::Message;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_recording_protocol::v1::{RecordingBatch, RecordingBlueprint};

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
    pub next_enqueue_blueprint_revision: u64,
    pub next_upload_blueprint_revision: u64,
    pub finish_requested: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueuedBatch {
    pub local_sequence: u64,
    pub batch: RecordingBatch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QueuedBlueprint {
    pub revision: u64,
    pub blueprint: RecordingBlueprint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueDiagnostics {
    pub queued_bytes: u64,
    pub maximum_bytes: u64,
    pub stream_count: usize,
    pub open_stream_count: usize,
    pub pending_batch_count: u64,
    pub pending_blueprint_count: u64,
    pub finishing_stream_count: usize,
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

    pub fn enqueue_blueprint(
        &mut self,
        application_id: &str,
        recording_id: &str,
        blueprint: &RecordingBlueprint,
    ) -> Result<(QueueStream, u64)> {
        validate_identity(application_id, recording_id)?;
        let added_bytes = u64::try_from(blueprint.encoded_len())?;
        if self.queued_bytes.saturating_add(added_bytes) > self.maximum_bytes {
            return Err(QueueFull.into());
        }
        let key = stream_key(application_id, recording_id);
        let directory = self.root.join(&key);
        std::fs::create_dir_all(&directory)?;
        sync_directory(&self.root)?;
        let mut stream = self.load_or_create_stream(&key, application_id, recording_id)?;
        let revision = stream.next_enqueue_blueprint_revision;
        let mut blueprint = blueprint.clone();
        blueprint.revision = revision;
        let path = blueprint_path(&directory, revision);
        let created = if path.exists() {
            let existing = RecordingBlueprint::decode(std::fs::read(&path)?.as_slice())?;
            ensure!(
                existing == blueprint,
                "queued Blueprint revision has conflicting content"
            );
            false
        } else {
            atomic_write(&path, &blueprint.encode_to_vec())?;
            true
        };
        stream.next_enqueue_blueprint_revision = revision
            .checked_add(1)
            .context("Blueprint revision overflow")?;
        self.write_stream(&stream)?;
        if created {
            self.queued_bytes = self.queued_bytes.saturating_add(added_bytes);
        }
        Ok((stream, revision))
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

    pub fn diagnostics(&self) -> Result<QueueDiagnostics> {
        let streams = self.streams()?;
        Ok(QueueDiagnostics {
            queued_bytes: self.queued_bytes,
            maximum_bytes: self.maximum_bytes,
            stream_count: streams.len(),
            open_stream_count: streams
                .iter()
                .filter(|stream| stream.remote_stream_id.is_some())
                .count(),
            pending_batch_count: streams.iter().fold(0_u64, |total, stream| {
                total.saturating_add(
                    stream
                        .next_enqueue_sequence
                        .saturating_sub(stream.next_upload_sequence),
                )
            }),
            pending_blueprint_count: streams.iter().fold(0_u64, |total, stream| {
                total.saturating_add(
                    stream
                        .next_enqueue_blueprint_revision
                        .saturating_sub(stream.next_upload_blueprint_revision),
                )
            }),
            finishing_stream_count: streams
                .iter()
                .filter(|stream| stream.finish_requested)
                .count(),
        })
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

    pub fn next_blueprint(&self, stream: &QueueStream) -> Result<Option<QueuedBlueprint>> {
        validate_key(&stream.key)?;
        if stream.next_upload_blueprint_revision >= stream.next_enqueue_blueprint_revision {
            return Ok(None);
        }
        let revision = stream.next_upload_blueprint_revision;
        let path = blueprint_path(&self.root.join(&stream.key), revision);
        let bytes = std::fs::read(&path)
            .with_context(|| format!("reading queued Blueprint {}", path.display()))?;
        let blueprint = RecordingBlueprint::decode(bytes.as_slice())
            .with_context(|| format!("decoding queued Blueprint {}", path.display()))?;
        Ok(Some(QueuedBlueprint {
            revision,
            blueprint,
        }))
    }

    pub fn has_blueprints(&self, stream: &QueueStream) -> Result<bool> {
        validate_key(&stream.key)?;
        Ok(stream.next_upload_blueprint_revision < stream.next_enqueue_blueprint_revision)
    }

    pub fn has_pending(&self, stream: &QueueStream) -> Result<bool> {
        Ok(self.has_batches(stream)? || self.has_blueprints(stream)?)
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

    pub fn acknowledge_blueprint(
        &mut self,
        stream: &QueueStream,
        revision: u64,
    ) -> Result<QueueStream> {
        let mut current = self.read_stream(&stream.key)?;
        ensure!(
            current.next_upload_blueprint_revision == revision,
            "acknowledged Blueprint is not the next queued revision"
        );
        let path = blueprint_path(&self.root.join(&stream.key), revision);
        ensure!(path.exists(), "acknowledged Blueprint is not queued");
        let byte_len = std::fs::metadata(&path)?.len();
        std::fs::remove_file(&path)?;
        sync_directory(path.parent().context("Blueprint path has no parent")?)?;
        current.next_upload_blueprint_revision = revision
            .checked_add(1)
            .context("Blueprint revision overflow")?;
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
            !self.has_pending(stream)?,
            "cannot complete a queued stream with pending data"
        );
        let directory = self.root.join(&stream.key);
        std::fs::remove_file(directory.join("stream.json"))?;
        sync_directory(&directory)?;
        std::fs::remove_dir(&directory)?;
        sync_directory(&self.root)
    }

    pub fn is_empty(&self) -> Result<bool> {
        Ok(self
            .streams()?
            .iter()
            .all(|stream| self.has_pending(stream).is_ok_and(|pending| !pending)))
    }

    fn reconcile(&mut self) -> Result<()> {
        let mut queued_bytes = 0_u64;
        for stream in self.streams()? {
            let inventory = self.batch_inventory(&stream)?;
            let blueprint_inventory = self.blueprint_inventory(&stream)?;
            queued_bytes = queued_bytes
                .saturating_add(inventory.byte_len)
                .saturating_add(blueprint_inventory.byte_len);
            let next_enqueue_sequence = inventory
                .last_sequence
                .map(|sequence| sequence.saturating_add(1))
                .unwrap_or(stream.next_enqueue_sequence)
                .max(stream.next_enqueue_sequence);
            let next_upload_sequence = inventory.first_sequence.unwrap_or(next_enqueue_sequence);
            let next_enqueue_blueprint_revision = blueprint_inventory
                .last_sequence
                .map(|revision| revision.saturating_add(1))
                .unwrap_or(stream.next_enqueue_blueprint_revision)
                .max(stream.next_enqueue_blueprint_revision);
            let next_upload_blueprint_revision = blueprint_inventory
                .first_sequence
                .unwrap_or(next_enqueue_blueprint_revision);
            if next_enqueue_sequence != stream.next_enqueue_sequence
                || next_upload_sequence != stream.next_upload_sequence
                || next_enqueue_blueprint_revision != stream.next_enqueue_blueprint_revision
                || next_upload_blueprint_revision != stream.next_upload_blueprint_revision
            {
                let mut repaired = stream;
                repaired.next_enqueue_sequence = next_enqueue_sequence;
                repaired.next_upload_sequence = next_upload_sequence;
                repaired.next_enqueue_blueprint_revision = next_enqueue_blueprint_revision;
                repaired.next_upload_blueprint_revision = next_upload_blueprint_revision;
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
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                anyhow::bail!("queued batch filename is not UTF-8");
            };
            if stem.starts_with("blueprint-") {
                continue;
            }
            let sequence = stem
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

    fn blueprint_inventory(&self, stream: &QueueStream) -> Result<BatchInventory> {
        validate_key(&stream.key)?;
        let mut inventory = BatchInventory::default();
        for entry in std::fs::read_dir(self.root.join(&stream.key))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("pb") {
                continue;
            }
            let Some(revision) = path
                .file_stem()
                .and_then(|value| value.to_str())
                .and_then(|value| value.strip_prefix("blueprint-"))
                .and_then(|value| value.parse::<u64>().ok())
            else {
                continue;
            };
            inventory.first_sequence = Some(
                inventory
                    .first_sequence
                    .map_or(revision, |current| current.min(revision)),
            );
            inventory.last_sequence = Some(
                inventory
                    .last_sequence
                    .map_or(revision, |current| current.max(revision)),
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
            next_enqueue_blueprint_revision: 1,
            next_upload_blueprint_revision: 1,
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

fn blueprint_path(directory: &Path, revision: u64) -> PathBuf {
    directory.join(format!("blueprint-{revision:020}.pb"))
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
    use veoveo_recording_protocol::v1::{RecordingBlueprint, RerunPayloadFormat};

    use super::*;

    fn batch() -> RecordingBatch {
        let payload = b"complete-rrd".to_vec();
        RecordingBatch {
            sequence: 0,
            payload_format: RerunPayloadFormat::Rrd0350.into(),
            sha256: Sha256::digest(&payload).to_vec(),
            encoded_rrd: payload,
            message_count: 1,
        }
    }

    fn blueprint() -> RecordingBlueprint {
        let payload = b"complete-blueprint-rrd".to_vec();
        RecordingBlueprint {
            revision: 0,
            payload_format: RerunPayloadFormat::Rrd0350.into(),
            sha256: Sha256::digest(&payload).to_vec(),
            encoded_rrd: payload,
            message_count: 3,
        }
    }

    #[test]
    fn blueprint_queue_restarts_and_acknowledges_each_revision_once() {
        let temporary = TempDir::new().unwrap();
        let root = temporary.path().join("queue");
        let mut queue = DurableQueue::open(root.clone(), 1_000_000).unwrap();
        let (_, first) = queue
            .enqueue_blueprint("camera", "run-a", &blueprint())
            .unwrap();
        let (_, second) = queue
            .enqueue_blueprint("camera", "run-a", &blueprint())
            .unwrap();
        assert_eq!((first, second), (1, 2));
        drop(queue);

        let mut queue = DurableQueue::open(root, 1_000_000).unwrap();
        let stream = queue.streams().unwrap().remove(0);
        let queued = queue.next_blueprint(&stream).unwrap().unwrap();
        assert_eq!(queued.revision, 1);
        assert_eq!(queued.blueprint.revision, 1);
        let stream = queue.acknowledge_blueprint(&stream, 1).unwrap();
        assert_eq!(queue.next_blueprint(&stream).unwrap().unwrap().revision, 2);
        let stream = queue.acknowledge_blueprint(&stream, 2).unwrap();
        assert!(!queue.has_blueprints(&stream).unwrap());
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
    fn diagnostics_report_bounded_backlog_without_stream_identity() {
        let temporary = TempDir::new().unwrap();
        let mut queue = DurableQueue::open(temporary.path().join("queue"), 1_000_000).unwrap();
        let (stream, first) = queue.enqueue("camera", "run-a", &batch()).unwrap();
        queue.enqueue("camera", "run-a", &batch()).unwrap();
        queue
            .enqueue_blueprint("camera", "run-a", &blueprint())
            .unwrap();
        queue.request_finish_all().unwrap();

        let diagnostics = queue.diagnostics().unwrap();
        assert_eq!(diagnostics.stream_count, 1);
        assert_eq!(diagnostics.open_stream_count, 0);
        assert_eq!(diagnostics.pending_batch_count, 2);
        assert_eq!(diagnostics.pending_blueprint_count, 1);
        assert_eq!(diagnostics.finishing_stream_count, 1);
        assert!(diagnostics.queued_bytes > 0);
        assert_eq!(diagnostics.maximum_bytes, 1_000_000);
        assert!(
            !serde_json::to_string(&diagnostics)
                .unwrap()
                .contains("run-a")
        );

        queue.acknowledge(&stream, first).unwrap();
        assert_eq!(queue.diagnostics().unwrap().pending_batch_count, 1);
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
