use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use re_log_channel::{DataSourceMessage, RecvTimeoutError};
use veoveo_platform_store::RecordingId;
use veoveo_recording_hub::{
    EncodedVideoClip, VideoClipRequest, extract_video_clip, extract_video_clip_from_messages,
    remux_h264_mp4,
};
use veoveo_recording_mcp::{RecordingReadAuthority, RecordingService};

use crate::contract::{AnalysisSource, RecordingVideoSelection};

#[derive(Clone, Debug)]
pub struct VideoSourceLimits {
    pub max_samples: usize,
    pub max_encoded_bytes: u64,
    pub max_segment_bytes: u64,
    pub max_recent_messages: usize,
    pub max_recent_capture: Duration,
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
        ensure!(
            self.max_recent_messages > 0,
            "max_recent_messages must be non-zero"
        );
        ensure!(
            self.max_recent_capture > Duration::ZERO,
            "max_recent_capture must be positive"
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
    proxy_uri: String,
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
    let clip = match selection.source {
        AnalysisSource::Durable => {
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
            tokio::task::spawn_blocking(move || extract_video_clip(&paths, &request))
                .await
                .context("video extraction worker panicked")??
        }
        AnalysisSource::RecentProxy {
            idle_ms,
            capture_ms,
        } => {
            let idle = Duration::from_millis(idle_ms);
            let capture = Duration::from_millis(capture_ms);
            ensure!(
                idle > Duration::ZERO,
                "recent proxy idle_ms must be positive"
            );
            ensure!(
                capture >= idle && capture <= limits.max_recent_capture,
                "recent proxy capture_ms is outside the allowed range"
            );
            collect_recent_clip(
                proxy_uri,
                request,
                idle,
                capture,
                limits.max_recent_messages,
            )
            .await?
        }
    };
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

async fn collect_recent_clip(
    proxy_uri: String,
    request: VideoClipRequest,
    idle: Duration,
    capture: Duration,
    max_messages: usize,
) -> Result<EncodedVideoClip> {
    ensure!(max_messages > 0, "max_recent_messages must be non-zero");
    let uri: re_uri::ProxyUri = proxy_uri
        .parse()
        .with_context(|| format!("invalid recording proxy URI `{proxy_uri}`"))?;
    let receiver = re_grpc_client::stream(uri);
    tokio::task::spawn_blocking(move || {
        let started = Instant::now();
        let mut last_match = None;
        let mut messages = Vec::new();
        loop {
            if started.elapsed() >= capture {
                break;
            }
            match receiver.recv_timeout(Duration::from_millis(100)) {
                Ok(message) => {
                    let Some(DataSourceMessage::LogMsg(message)) = message.into_data() else {
                        continue;
                    };
                    if message.store_id().is_recording()
                        && message.store_id().application_id().as_str() == request.application_id
                        && message.store_id().recording_id().as_str() == request.recording_key
                    {
                        ensure!(
                            messages.len() < max_messages,
                            "recent recording replay exceeds max_recent_messages"
                        );
                        last_match = Some(Instant::now());
                        messages.push(message);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if last_match.is_some_and(|last| last.elapsed() >= idle) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        ensure!(
            !messages.is_empty(),
            "recording proxy replay contained no authorized recording messages"
        );
        extract_video_clip_from_messages(messages, &request)
    })
    .await
    .context("recent proxy collection worker panicked")?
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
