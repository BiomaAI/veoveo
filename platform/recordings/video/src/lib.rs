//! Governed recorded-video access shared by every server that consumes Rerun
//! `VideoStream` recordings.
//!
//! One selection contract, one bounded materialization path: authorize the
//! canonical recording identity, read only frozen or sealed segments through
//! the recording read plan, extract the requested range from the preceding
//! decoder-reentrant keyframe, and remux to MP4 without re-encoding. Servers
//! own what happens to the materialized clip; this crate owns how recorded
//! video is reached.

use std::sync::Arc;

use anyhow::{Context, Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_platform_store::RecordingId;
use veoveo_recording_hub::{
    EncodedVideoClip, VideoClipRequest, VideoIndexKind, extract_video_clip, remux_h264_mp4,
};
use veoveo_recording_mcp::{RecordingReadAuthority, RecordingService};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingVideoSelection {
    /// Canonical `recording://recordings/{recording_id}` URI.
    pub recording_uri: String,
    /// Exact Rerun entity path containing `VideoStream` samples.
    pub entity_path: String,
    /// Rerun duration, timestamp, or sequence timeline.
    pub timeline: String,
    pub range: IndexRange,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexRange {
    pub start: i64,
    pub end: i64,
}

impl IndexRange {
    pub fn contains(&self, other: IndexRange) -> bool {
        other.start >= self.start && other.end <= self.end
    }
}

/// Timeline kinds a remuxed clip can preserve. Sequence timelines carry no
/// media time and are rejected at materialization mapping.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum VideoTimelineKind {
    DurationNanoseconds,
    TimestampNanoseconds,
}

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
    pub clip: EncodedVideoClip,
    pub mp4: Vec<u8>,
}

pub async fn materialize_video(
    recordings: Arc<RecordingService>,
    authority: RecordingReadAuthority,
    selection: RecordingVideoSelection,
    limits: VideoSourceLimits,
) -> Result<MaterializedVideo> {
    limits.validate()?;
    validate_video_selection(&selection)?;
    let recording_id = recording_id_from_uri(&selection.recording_uri)?;
    let plan = recordings
        .read_plan(&authority, recording_id)
        .await?
        .context("recording not found")?;
    let request = VideoClipRequest {
        application_id: plan.application_id.clone(),
        recording_key: plan.recording_key.clone(),
        entity_path: selection.entity_path.clone(),
        timeline: selection.timeline.clone(),
        start_index: selection.range.start,
        end_index: selection.range.end,
        max_samples: limits.max_samples,
        max_encoded_bytes: limits.max_encoded_bytes,
    };
    let segment_bytes = plan
        .segments
        .iter()
        .filter(|segment| {
            matches!(
                segment.state,
                veoveo_platform_store::SegmentState::Frozen
                    | veoveo_platform_store::SegmentState::Sealed
            )
        })
        .try_fold(0_u64, |total, segment| {
            total
                .checked_add(segment.byte_len)
                .context("recording segment byte count overflow")
        })?;
    ensure!(
        segment_bytes <= limits.max_segment_bytes,
        "authorized recording exceeds max_segment_bytes ({})",
        limits.max_segment_bytes
    );
    let paths = plan.stable_segment_paths();
    ensure!(
        !paths.is_empty(),
        "recording has no frozen or sealed segments"
    );
    let clip = tokio::task::spawn_blocking(move || extract_video_clip(&paths, &request))
        .await
        .context("video extraction worker panicked")??;
    let clip_for_remux = clip.clone();
    let mp4 = tokio::task::spawn_blocking(move || remux_h264_mp4(&clip_for_remux))
        .await
        .context("video remux worker panicked")??;
    Ok(MaterializedVideo {
        recording_id,
        application_id: plan.application_id,
        recording_key: plan.recording_key,
        classification: plan.classification,
        labels: plan.labels,
        clip,
        mp4,
    })
}

pub fn validate_video_selection(selection: &RecordingVideoSelection) -> Result<()> {
    ensure!(
        selection.range.start <= selection.range.end,
        "video range start must not exceed end"
    );
    ensure!(
        selection.entity_path.starts_with('/')
            && selection.entity_path.len() <= 4_096
            && !selection.entity_path.chars().any(char::is_control),
        "entity_path must be an absolute Rerun path"
    );
    ensure!(
        !selection.timeline.is_empty()
            && selection.timeline.len() <= 256
            && !selection.timeline.chars().any(char::is_control),
        "timeline is invalid"
    );
    Ok(())
}

pub fn recording_id_from_uri(uri: &str) -> Result<RecordingId> {
    let value = veoveo_recording_mcp::uris::parse_recording_uri(uri)
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

    fn selection() -> RecordingVideoSelection {
        RecordingVideoSelection {
            recording_uri: "recording://recordings/01983da0-0000-7000-8000-000000000000".to_owned(),
            entity_path: "/camera/front".to_owned(),
            timeline: "sensor_time".to_owned(),
            range: IndexRange { start: 0, end: 10 },
        }
    }

    #[test]
    fn selection_validation_rejects_inverted_ranges() {
        let mut inverted = selection();
        inverted.range = IndexRange { start: 5, end: 4 };
        assert!(validate_video_selection(&inverted).is_err());
        assert!(validate_video_selection(&selection()).is_ok());
    }

    #[test]
    fn selection_validation_rejects_relative_entity_paths() {
        let mut relative = selection();
        relative.entity_path = "camera/front".to_owned();
        assert!(validate_video_selection(&relative).is_err());
    }

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

    #[test]
    fn index_ranges_contain_their_subranges() {
        let outer = IndexRange { start: 0, end: 10 };
        assert!(outer.contains(IndexRange { start: 0, end: 10 }));
        assert!(outer.contains(IndexRange { start: 3, end: 7 }));
        assert!(!outer.contains(IndexRange { start: 3, end: 11 }));
    }
}
