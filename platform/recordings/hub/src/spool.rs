//! The durable write path: demux incoming `LogMsg`s into day-partitioned
//! segment files, one file per `(dataset, day, recording)`, and freeze them on
//! size/age so the catalog can lazy-load their manifests.
//!
//! Files are footer-less while live (crash-decodable to the last whole
//! message) and gain a footer/manifest at freeze. On restart a recording that
//! already has a live file resumes into a fresh `.rN` sibling — an RRD file is
//! never truncated in place, so a crashed segment is always recoverable.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use re_build_info::CrateVersion;
use re_log_channel::{DataSourceMessage, LogReceiver, RecvTimeoutError};
use re_log_encoding::EncodingOptions;
use re_log_encoding::rrd::Encoder;
use re_log_types::{LogMsg, StoreKind};

use crate::config::{DatasetName, SpoolerConfig};

/// Durable catalog hooks invoked at segment lifecycle boundaries. A hook
/// failure is fatal to the drain: bytes remain on disk, but the proxy stops
/// accepting traffic rather than running with an unauthoritative catalog.
pub trait SegmentCatalog: Send {
    fn segment_opened(&mut self, segment: &OpenedSegment) -> Result<()>;
    fn segment_frozen(&mut self, segment: &FrozenSegment) -> Result<()>;
    fn recording_finished(&mut self, key: &SegmentKey, ended_at: DateTime<Utc>) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct OpenedSegment {
    pub key: SegmentKey,
    pub path: PathBuf,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct FrozenSegment {
    pub key: SegmentKey,
    pub path: PathBuf,
    pub byte_len: u64,
    pub message_count: u64,
    pub sha256: String,
    pub ended_at: DateTime<Utc>,
}

#[derive(Default)]
struct NoopCatalog;

impl SegmentCatalog for NoopCatalog {
    fn segment_opened(&mut self, _segment: &OpenedSegment) -> Result<()> {
        Ok(())
    }

    fn segment_frozen(&mut self, _segment: &FrozenSegment) -> Result<()> {
        Ok(())
    }

    fn recording_finished(&mut self, _key: &SegmentKey, _ended_at: DateTime<Utc>) -> Result<()> {
        Ok(())
    }
}

/// The identity of one segment file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SegmentKey {
    pub dataset: DatasetName,
    pub day: NaiveDate,
    pub application_id: String,
    pub recording: String,
}

/// Aggregate counters, exported for logging and the bench harness.
#[derive(Debug, Default, Clone)]
pub struct Counters {
    pub messages: u64,
    pub bytes: u64,
    pub segments_opened: u64,
    pub segments_frozen: u64,
    pub quarantined: u64,
}

struct SegmentWriter {
    path: PathBuf,
    encoder: Option<Encoder<BufWriter<File>>>,
    sync_file: File,
    opened_at: Instant,
    last_message_at: Instant,
    started_at: DateTime<Utc>,
    last_data_at: DateTime<Utc>,
    bytes: u64,
    messages: u64,
}

impl SegmentWriter {
    fn create(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating segment dir {}", parent.display()))?;
        }
        let file = File::create(&path)
            .with_context(|| format!("creating segment file {}", path.display()))?;
        sync_parent_dir(&path)?;
        let sync_file = file
            .try_clone()
            .with_context(|| format!("cloning segment file {}", path.display()))?;
        let encoder = Encoder::new_eager(
            CrateVersion::LOCAL,
            EncodingOptions::PROTOBUF_COMPRESSED,
            BufWriter::with_capacity(1024 * 1024, file),
        )
        .with_context(|| format!("opening encoder for {}", path.display()))?;
        let now = Utc::now();
        Ok(Self {
            path,
            encoder: Some(encoder),
            sync_file,
            opened_at: Instant::now(),
            last_message_at: Instant::now(),
            started_at: now,
            last_data_at: now,
            bytes: 0,
            messages: 0,
        })
    }

    fn append(&mut self, msg: &LogMsg) -> Result<u64> {
        let encoder = self
            .encoder
            .as_mut()
            .context("append to a frozen segment writer")?;
        let n = encoder
            .append(msg)
            .with_context(|| format!("appending to {}", self.path.display()))?;
        self.bytes += n;
        self.messages += 1;
        self.last_message_at = Instant::now();
        self.last_data_at = Utc::now();
        Ok(n)
    }

    fn append_preamble(&mut self, msg: &LogMsg) -> Result<()> {
        let encoder = self
            .encoder
            .as_mut()
            .context("append preamble to a frozen segment writer")?;
        let n = encoder
            .append(msg)
            .with_context(|| format!("appending preamble to {}", self.path.display()))?;
        self.bytes += n;
        Ok(())
    }

    fn flush(&mut self, fsync: bool) -> Result<()> {
        if let Some(encoder) = self.encoder.as_mut() {
            encoder
                .flush_blocking()
                .with_context(|| format!("flushing {}", self.path.display()))?;
        }
        if fsync {
            self.sync_file
                .sync_data()
                .with_context(|| format!("syncing {}", self.path.display()))?;
        }
        Ok(())
    }

    /// Write the footer/manifest and release the file.
    fn finish(&mut self, fsync: bool) -> Result<()> {
        if let Some(mut encoder) = self.encoder.take() {
            encoder
                .finish()
                .with_context(|| format!("finishing {}", self.path.display()))?;
            encoder
                .flush_blocking()
                .with_context(|| format!("final flush {}", self.path.display()))?;
        }
        if fsync {
            self.sync_file
                .sync_data()
                .with_context(|| format!("final sync {}", self.path.display()))?;
            sync_parent_dir(&self.path)?;
        }
        Ok(())
    }
}

/// Drives demux, freeze, and shutdown for all active recordings.
pub struct Spooler {
    config: SpoolerConfig,
    writers: HashMap<SegmentKey, SegmentWriter>,
    counters: Counters,
    today: fn() -> NaiveDate,
    catalog: Box<dyn SegmentCatalog>,
    store_info: HashMap<(String, String), LogMsg>,
}

impl Spooler {
    pub fn new(config: SpoolerConfig) -> Result<Self> {
        config.validate()?;
        std::fs::create_dir_all(&config.spool_dir)
            .with_context(|| format!("creating spool dir {}", config.spool_dir.display()))?;
        Ok(Self {
            config,
            writers: HashMap::new(),
            counters: Counters::default(),
            today: || chrono::Utc::now().date_naive(),
            catalog: Box::new(NoopCatalog),
            store_info: HashMap::new(),
        })
    }

    pub fn with_catalog(mut self, catalog: impl SegmentCatalog + 'static) -> Self {
        self.catalog = Box::new(catalog);
        self
    }

    /// Override the clock (tests inject a fixed day).
    pub fn with_clock(mut self, today: fn() -> NaiveDate) -> Self {
        self.today = today;
        self
    }

    pub fn counters(&self) -> &Counters {
        &self.counters
    }

    pub fn spool_dir(&self) -> &Path {
        &self.config.spool_dir
    }

    /// Route and persist one message. Blueprint stores are ignored (they are
    /// viewer UI state, not recorded world data).
    pub fn ingest(&mut self, msg: &LogMsg) -> Result<()> {
        let store_id = msg.store_id();
        if store_id.kind() != StoreKind::Recording {
            return Ok(());
        }
        let application_id = store_id.application_id().as_str().to_owned();
        let recording = store_id.recording_id().as_str().to_owned();
        let store_key = (application_id.clone(), recording.clone());
        if matches!(msg, LogMsg::SetStoreInfo(_)) {
            self.store_info.insert(store_key.clone(), msg.clone());
        }
        let dataset = self.config.dataset_for(&application_id);
        if dataset.as_str() == crate::config::QUARANTINE_DATASET {
            self.counters.quarantined += 1;
        }
        let key = SegmentKey {
            dataset,
            day: (self.today)(),
            application_id,
            recording,
        };

        if !self.writers.contains_key(&key) {
            let path = self.next_segment_path(&key)?;
            let mut writer = SegmentWriter::create(path.clone())?;
            if !matches!(msg, LogMsg::SetStoreInfo(_))
                && let Some(store_info) = self.store_info.get(&store_key)
            {
                writer.append_preamble(store_info)?;
            }
            self.catalog.segment_opened(&OpenedSegment {
                key: key.clone(),
                path,
                started_at: writer.started_at,
            })?;
            self.counters.segments_opened += 1;
            self.writers.insert(key.clone(), writer);
        }
        let writer = self.writers.get_mut(&key).expect("writer just inserted");
        let n = writer.append(msg)?;
        self.counters.messages += 1;
        self.counters.bytes += n;
        Ok(())
    }

    /// Flush every live segment's buffered bytes to the OS.
    pub fn flush_all(&mut self) -> Result<()> {
        for writer in self.writers.values_mut() {
            writer.flush(self.config.fsync_on_flush)?;
        }
        Ok(())
    }

    /// Freeze segments that have outgrown their size or age budget, opening a
    /// fresh `.rN` sibling for any continued traffic on that key.
    pub fn freeze_due(&mut self) -> Result<()> {
        let idle_timeout = self.config.recording_idle_timeout();
        let idle: Vec<SegmentKey> = self
            .writers
            .iter()
            .filter(|(_, writer)| writer.last_message_at.elapsed() >= idle_timeout)
            .map(|(key, _)| key.clone())
            .collect();
        for key in idle {
            self.freeze_key(&key, true)?;
        }
        let max_bytes = self.config.segment_max_bytes;
        let max_age = self.config.segment_max_age();
        let due: Vec<SegmentKey> = self
            .writers
            .iter()
            .filter(|(_, w)| w.bytes >= max_bytes || w.opened_at.elapsed() >= max_age)
            .map(|(k, _)| k.clone())
            .collect();
        for key in due {
            self.freeze_key(&key, false)?;
        }
        Ok(())
    }

    /// Freeze and close every segment at shutdown, recording a clean capture
    /// boundary. A later producer reconnect resumes the same recording.
    pub fn freeze_all(&mut self) -> Result<()> {
        let keys: Vec<SegmentKey> = self.writers.keys().cloned().collect();
        for key in keys {
            self.freeze_key(&key, true)?;
        }
        Ok(())
    }

    fn freeze_key(&mut self, key: &SegmentKey, finish_recording: bool) -> Result<()> {
        if let Some(mut writer) = self.writers.remove(key) {
            writer.finish(self.config.fsync_on_flush)?;
            self.counters.segments_frozen += 1;
            // Compact + embed a manifest so the catalog can lazy-load the
            // segment, then verify. Both are best-effort: a freeze that can't
            // reach the CLI still leaves a valid footer-full file behind.
            if let Some(bin) = self.config.rerun_bin.clone()
                && let Err(err) = optimize_segment(&bin, &writer.path)
            {
                tracing::warn!(%err, path = %writer.path.display(), "segment optimize failed");
            }
            let inspection = crate::catalog::inspect_segment(&writer.path)?;
            anyhow::ensure!(
                inspection.application_id == key.application_id
                    && inspection.recording_key == key.recording,
                "frozen segment identity changed while writing {}",
                writer.path.display()
            );
            self.catalog.segment_frozen(&FrozenSegment {
                key: key.clone(),
                path: writer.path.clone(),
                byte_len: inspection.byte_len,
                message_count: writer.messages,
                sha256: inspection.sha256,
                ended_at: writer.last_data_at,
            })?;
            if finish_recording {
                self.catalog.recording_finished(key, writer.last_data_at)?;
            }
            tracing::info!(
                dataset = key.dataset.as_str(),
                recording = key.recording,
                path = %writer.path.display(),
                messages = writer.messages,
                bytes = writer.bytes,
                "segment frozen"
            );
        }
        Ok(())
    }

    /// Pick a not-yet-existing path for a segment key, so a crashed prior file
    /// is never truncated: `{recording}.rrd`, then `{recording}.r1.rrd`, ...
    fn next_segment_path(&self, key: &SegmentKey) -> Result<PathBuf> {
        let dir = self
            .config
            .spool_dir
            .join(key.dataset.as_str())
            .join(key.day.format("%Y-%m-%d").to_string());
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("creating dataset dir {}", dir.display()))?;
        let base = dir.join(format!("{}.rrd", sanitize(&key.recording)));
        if !base.exists() {
            return Ok(base);
        }
        for n in 1.. {
            let candidate = dir.join(format!("{}.r{n}.rrd", sanitize(&key.recording)));
            if !candidate.exists() {
                return Ok(candidate);
            }
        }
        unreachable!("infinite range yields an unused path")
    }
}

/// Make a recording id safe as a filename component.
fn sanitize(recording: &str) -> String {
    recording
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Drain a proxy receiver into a spooler until `stop` is set or the channel
/// disconnects, flushing and freezing on a cadence, then freeze everything.
///
/// This is the shared write loop: the `spooler` binary runs it on a blocking
/// thread, and integration tests run it the same way, so the two never drift.
pub fn run_blocking(
    mut spooler: Spooler,
    receiver: LogReceiver,
    stop: Arc<AtomicBool>,
    flush_interval: Duration,
    counters_interval: Duration,
) -> Result<Counters> {
    let mut last_flush = Instant::now();
    let mut last_counters = Instant::now();

    loop {
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(msg) => {
                if let Some(DataSourceMessage::LogMsg(log_msg)) = msg.into_data() {
                    spooler.ingest(&log_msg)?;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if last_flush.elapsed() >= flush_interval {
            spooler.flush_all()?;
            spooler.freeze_due()?;
            last_flush = Instant::now();
        }
        if last_counters.elapsed() >= counters_interval {
            let c = spooler.counters();
            tracing::info!(
                messages = c.messages,
                bytes = c.bytes,
                segments_opened = c.segments_opened,
                segments_frozen = c.segments_frozen,
                quarantined = c.quarantined,
                "hub counters"
            );
            last_counters = Instant::now();
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
    }

    // Drain anything still queued, then freeze everything durably.
    while let Ok(msg) = receiver.try_recv() {
        if let Some(DataSourceMessage::LogMsg(log_msg)) = msg.into_data() {
            spooler.ingest(&log_msg)?;
        }
    }
    spooler.flush_all()?;
    spooler.freeze_all()?;
    Ok(spooler.counters().clone())
}

fn verify_segment(rerun_bin: &Path, path: &Path) -> Result<()> {
    let output = std::process::Command::new(rerun_bin)
        .arg("rrd")
        .arg("verify")
        .arg(path)
        .output()
        .with_context(|| format!("running {} rrd verify", rerun_bin.display()))?;
    anyhow::ensure!(
        output.status.success(),
        "rrd verify failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn sync_parent_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("segment path {} has no parent", path.display()))?;
    File::open(parent)
        .with_context(|| format!("opening segment directory {}", parent.display()))?
        .sync_all()
        .with_context(|| format!("syncing segment directory {}", parent.display()))
}

/// Compact a frozen segment in place: `rerun rrd optimize <path> -o <tmp>` then
/// atomically rename over the original. The compacted file carries the manifest
/// the catalog needs to lazy-load it. A crash mid-optimize leaves the original
/// untouched and a stray `.opt` to sweep.
fn optimize_segment(rerun_bin: &Path, path: &Path) -> Result<()> {
    let tmp = path.with_extension("rrd.opt");
    let output = std::process::Command::new(rerun_bin)
        .arg("rrd")
        .arg("optimize")
        .arg(path)
        .arg("-o")
        .arg(&tmp)
        .output()
        .with_context(|| format!("running {} rrd optimize", rerun_bin.display()))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!(
            "rrd optimize failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if let Err(err) = verify_segment(rerun_bin, &tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(err).with_context(|| format!("verifying optimized segment {}", path.display()));
    }
    std::fs::rename(&tmp, path)
        .with_context(|| format!("publishing optimized segment {}", path.display()))?;
    sync_parent_dir(path)?;
    Ok(())
}
