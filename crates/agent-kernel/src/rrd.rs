//! The episodic memory plane: one logical Rerun recording, lived in segments.
//!
//! Every process boot opens a fresh footer-less segment file (`FileSink`
//! truncates on open, and the streaming footer manifest grows unboundedly on
//! long runs), all pinned to one recording id persisted in the kernel memory —
//! the viewer pools the segments into a single timeline. An optional gRPC tee
//! streams the same data to a live viewer. Rotation swaps in a new segment
//! once the live one exceeds the configured size.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use re_sdk::{
    RecordingStream, RecordingStreamBuilder,
    sink::{FileSink, FileSinkOptions, GrpcSink, LogSink},
};
use re_sdk_types::archetypes::{Scalars, TextDocument, TextLog};

use crate::memory::MemoryStore;

pub const EPISODE_TIMELINE: &str = "episode";
const RECORDING_ID_KEY: &str = "recording_id";

pub struct RrdRecorder {
    stream: RecordingStream,
    application_id: String,
    recording_id: String,
    rrd_dir: PathBuf,
    segment_path: std::sync::Mutex<PathBuf>,
    segment_max_bytes: u64,
    viewer_tee: Option<String>,
}

impl RrdRecorder {
    pub fn open(
        data_dir: &Path,
        rrd_dir_name: &str,
        segment_max_bytes: u64,
        agent_id: &str,
        memory: &MemoryStore,
        viewer_tee: Option<String>,
    ) -> Result<Self> {
        let rrd_dir = data_dir.join(rrd_dir_name);
        std::fs::create_dir_all(&rrd_dir)?;

        let application_id = format!("veoveo-agent-{agent_id}");
        let recording_id = match memory.kv_get(RECORDING_ID_KEY)? {
            Some(serde_json::Value::String(id)) => id,
            _ => {
                let id = format!("{application_id}-{}", uuid::Uuid::now_v7());
                memory.kv_set(RECORDING_ID_KEY, &serde_json::Value::String(id.clone()))?;
                id
            }
        };

        let segment_path =
            rrd_dir.join(format!("mem-{}.rrd", chrono::Utc::now().timestamp_millis()));
        let stream = RecordingStreamBuilder::new(application_id.as_str())
            .recording_id(recording_id.clone())
            .set_sinks(build_sinks(&segment_path, viewer_tee.as_deref())?)
            .context("opening RRD recording stream")?;
        tracing::info!(
            recording_id,
            segment = %segment_path.display(),
            tee = viewer_tee.as_deref().unwrap_or("off"),
            "rrd segment opened"
        );

        Ok(Self {
            stream,
            application_id,
            recording_id,
            rrd_dir,
            segment_path: std::sync::Mutex::new(segment_path),
            segment_max_bytes,
            viewer_tee,
        })
    }

    pub fn rrd_dir(&self) -> &Path {
        &self.rrd_dir
    }

    /// Set the episode timeline for everything logged until the next call.
    pub fn begin_episode(&self, seq: i64) {
        self.stream.set_time_sequence(EPISODE_TIMELINE, seq);
    }

    pub fn log_text(&self, entity_path: &str, text: impl Into<String>) {
        if let Err(err) = self.stream.log(entity_path, &TextLog::new(text.into())) {
            tracing::warn!(entity_path, %err, "rrd text log failed");
        }
    }

    /// A long-form document (episode prompts, structured JSON payloads).
    pub fn log_document(&self, entity_path: &str, media_type: &str, body: impl Into<String>) {
        let document = TextDocument::new(body.into()).with_media_type(media_type);
        if let Err(err) = self.stream.log(entity_path, &document) {
            tracing::warn!(entity_path, %err, "rrd document log failed");
        }
    }

    pub fn log_scalars(&self, entity_path: &str, values: impl IntoIterator<Item = f64>) {
        if let Err(err) = self.stream.log(entity_path, &Scalars::new(values)) {
            tracing::warn!(entity_path, %err, "rrd scalar log failed");
        }
    }

    pub fn flush(&self) {
        if let Err(err) = self.stream.flush_blocking() {
            tracing::warn!(%err, "rrd flush failed");
        }
    }

    /// Swap to a fresh segment when the live one has outgrown its budget.
    /// Cheap to call between episodes.
    pub fn rotate_if_needed(&self) -> Result<()> {
        let mut segment_path = self
            .segment_path
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let size = std::fs::metadata(&*segment_path)
            .map(|meta| meta.len())
            .unwrap_or(0);
        if size < self.segment_max_bytes {
            return Ok(());
        }
        self.flush();
        let next_path = self
            .rrd_dir
            .join(format!("mem-{}.rrd", chrono::Utc::now().timestamp_millis()));
        self.stream
            .set_sinks(build_sinks(&next_path, self.viewer_tee.as_deref())?);
        tracing::info!(
            recording_id = self.recording_id,
            application_id = self.application_id,
            previous = %segment_path.display(),
            segment = %next_path.display(),
            "rrd segment rotated"
        );
        *segment_path = next_path;
        Ok(())
    }
}

impl Drop for RrdRecorder {
    fn drop(&mut self) {
        if let Err(err) = self.stream.flush_blocking() {
            tracing::warn!(%err, "rrd flush on drop failed");
        }
    }
}

fn build_sinks(segment_path: &Path, viewer_tee: Option<&str>) -> Result<Vec<Box<dyn LogSink>>> {
    let file = FileSink::with_options(
        segment_path.to_path_buf(),
        FileSinkOptions {
            write_footer: false,
        },
    )
    .with_context(|| format!("opening RRD segment {}", segment_path.display()))?;
    let mut sinks: Vec<Box<dyn LogSink>> = vec![Box::new(file)];
    if let Some(uri) = viewer_tee {
        let proxy_uri = uri
            .parse()
            .map_err(|err| anyhow::anyhow!("viewer tee uri `{uri}`: {err}"))?;
        sinks.push(Box::new(GrpcSink::new(proxy_uri)));
    }
    Ok(sinks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_share_the_persisted_recording_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let memory = MemoryStore::open(&dir.path().join("memory.duckdb")).expect("memory");

        let first = RrdRecorder::open(dir.path(), "rrd", 1024, "test", &memory, None)
            .expect("first recorder");
        let first_id = first.recording_id.clone();
        first.log_text("/agent/test", "hello");
        first.flush();
        drop(first);

        let second = RrdRecorder::open(dir.path(), "rrd", 1024, "test", &memory, None)
            .expect("second recorder");
        assert_eq!(second.recording_id, first_id);

        let segments: Vec<_> = std::fs::read_dir(dir.path().join("rrd"))
            .expect("rrd dir")
            .collect();
        assert!(!segments.is_empty());
    }
}
