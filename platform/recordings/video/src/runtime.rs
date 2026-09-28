//! Authorized source materialization and RRD/MP4 adapters.

use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use veoveo_mcp_contract::ArtifactReadAuthority;
use veoveo_platform_store::RecordingId;
use veoveo_recording_reader::{RecordingReadAuthority, RecordingReadSnapshot, RecordingReader};
use veoveo_rrd::video_clip::{
    EncodedVideoClip, VideoClipRequest, VideoIndexKind, extract_video_clip, remux_h264_mp4,
};

use crate::contract::{RecordingVideoSelection, VideoTimelineKind, validate_video_selection};

mod source_snapshot;

pub fn timeline_kind(clip: &EncodedVideoClip) -> Result<VideoTimelineKind> {
    match clip.index_kind {
        VideoIndexKind::DurationNanoseconds => Ok(VideoTimelineKind::DurationNanoseconds),
        VideoIndexKind::TimestampNanoseconds => Ok(VideoTimelineKind::TimestampNanoseconds),
        VideoIndexKind::Sequence => {
            bail!("sequence-indexed video cannot be remuxed for decoder input")
        }
    }
}

#[derive(Clone, Debug)]
pub struct VideoSourceLimits {
    pub max_samples: usize,
    pub max_encoded_bytes: u64,
    pub max_segment_bytes: u64,
}

impl VideoSourceLimits {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.max_samples > 0, "max_samples must be non-zero");
        ensure!(
            self.max_encoded_bytes > 0,
            "max_encoded_bytes must be non-zero"
        );
        ensure!(
            self.max_segment_bytes > 0,
            "max_segment_bytes must be non-zero"
        );
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct MaterializedVideo {
    pub recording_id: RecordingId,
    pub application_id: String,
    pub recording_key: String,
    pub classification: String,
    pub labels: Vec<String>,
    pub source_snapshot: RecordingReadSnapshot,
    pub clip: EncodedVideoClip,
    pub mp4: Vec<u8>,
}

pub async fn materialize_video(
    recordings: Arc<RecordingReader>,
    authority: RecordingReadAuthority,
    credential: ArtifactReadAuthority<'_>,
    selection: RecordingVideoSelection,
    limits: VideoSourceLimits,
) -> Result<MaterializedVideo> {
    limits.validate()?;
    validate_video_selection(&selection)?;
    let recording_id = recording_id_from_uri(&selection.recording_uri)?;
    let materialized = recordings
        .materialize_analysis_snapshot(
            &authority,
            credential,
            recording_id,
            limits.max_segment_bytes,
        )
        .await?
        .context("recording not found")?;
    let plan = &materialized.plan;
    let application_id = plan.application_id.clone();
    let recording_key = plan.recording_key.clone();
    let classification = plan.classification.clone();
    let labels = plan.labels.clone();
    let request = VideoClipRequest {
        application_id: application_id.clone(),
        recording_key: recording_key.clone(),
        entity_path: selection.entity_path.clone(),
        timeline: selection.timeline.clone(),
        start_index: selection.range.start,
        end_index: selection.range.end,
        max_samples: limits.max_samples,
        max_encoded_bytes: limits.max_encoded_bytes,
    };
    let (source_snapshot, clip, mp4) = tokio::task::spawn_blocking(move || {
        let source_bytes =
            materialized
                .snapshot
                .sources
                .iter()
                .try_fold(0_u64, |total, source| {
                    total
                        .checked_add(source.byte_len)
                        .context("recording source byte count overflow")
                })?;
        ensure!(
            source_bytes <= limits.max_segment_bytes,
            "authorized recording snapshot exceeds max_segment_bytes ({})",
            limits.max_segment_bytes
        );
        ensure!(
            !materialized.paths().is_empty(),
            "recording has no durable analysis sources"
        );
        let clip = extract_video_clip(materialized.paths(), &request)?;
        let mp4 = remux_h264_mp4(&clip)?;
        Ok::<_, anyhow::Error>((materialized.snapshot.clone(), clip, mp4))
    })
    .await
    .context("video materialization worker panicked")??;
    Ok(MaterializedVideo {
        recording_id,
        application_id,
        recording_key,
        classification,
        labels,
        source_snapshot,
        clip,
        mp4,
    })
}

pub fn recording_id_from_uri(uri: &str) -> Result<RecordingId> {
    let value = veoveo_recording_reader::uris::parse_recording_uri(uri)
        .context("recording_uri must match recording://recordings/{recording_id}")?;
    let value = uuid::Uuid::parse_str(value).context("recording URI id must be a UUIDv7")?;
    ensure!(
        value.get_version_num() == 7,
        "recording URI id must be a UUIDv7"
    );
    Ok(RecordingId::from_uuid(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_ids_must_be_canonical_uuidv7() {
        assert!(
            recording_id_from_uri("recording://recordings/01983da0-0000-7000-8000-000000000000")
                .is_ok()
        );
        assert!(
            recording_id_from_uri("recording://recordings/8c5e505e-3e18-4a4e-8f3b-000000000000")
                .is_err()
        );
        assert!(recording_id_from_uri("artifact://something").is_err());
    }
}
