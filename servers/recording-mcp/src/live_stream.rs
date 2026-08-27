//! Recording-scoped, framed RRD stream for Rerun WebViewer channels.
//!
//! Rerun 0.36's public `WebViewer.open_channel` API accepts complete RRD byte
//! arrays. This adapter preserves those decode boundaries over streaming HTTP:
//! every body frame is a four-byte big-endian length followed by one complete
//! RRD payload. Frames follow durable ingest and layer notifications; there
//! is no playback poll or delivery timer.

use std::{io, pin::Pin, time::Duration};

use anyhow::Context as _;
use bytes::{BufMut as _, Bytes, BytesMut};
use futures::{Stream, StreamExt as _};
use re_build_info::CrateVersion;
use re_log_encoding::{EncodingOptions, rrd::Encoder};
use re_log_types::{LogMsg, StoreId};
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_platform_store::{PlatformTable, RecordingId, RecordingLayerRecord, RecordingState};

use crate::{
    live_playback::{LiveMessageStart, stream_live_message_batches},
    service::{PlaybackLiveLayerPlan, RecordingService},
};

pub const FRAMED_RRD_CONTENT_TYPE: &str =
    "application/vnd.veoveo.rerun.rrd-stream; framing=be32; version=2";
pub const LIVE_RRD_START_HEADER: &str = "x-veoveo-rerun-live-start";
pub const MAX_RRD_FRAME_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveRrdStart {
    Bootstrap,
    ResumeHead,
}

impl LiveRrdStart {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "bootstrap" => Some(Self::Bootstrap),
            "resume-head" => Some(Self::ResumeHead),
            _ => None,
        }
    }
}

pub type LiveRrdStream = Pin<Box<dyn Stream<Item = Result<Bytes, io::Error>> + Send + 'static>>;

pub fn authorized_live_rrd_stream(
    recordings: RecordingService,
    identity: GatewayInternalIdentity,
    recording_id: RecordingId,
    history: Duration,
    playback_store_id: StoreId,
    start: LiveRrdStart,
) -> LiveRrdStream {
    let stream = async_stream::try_stream! {
        // Subscribe before projecting the current writing layer. A rollover racing
        // authorization remains queued as a typed wakeup rather than requiring a poll.
        let mut layer_wake = recordings
            .platform_store()
            .live::<RecordingLayerRecord>(PlatformTable::RecordingLayer)
            .await
            .map_err(|error| io::Error::other(format!("subscribe to recording layers: {error}")))?;
        let mut last_ordinal = None;
        let mut store_info_sent = false;
        let mut first_layer = true;

        loop {
            let next = loop {
                let plan = recordings
                    .playback_plan(&identity, None, recording_id)
                    .await
                    .map_err(|error| io::Error::other(format!("authorize live recording: {error}")))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "recording is not visible"))?;
                if let Some(live) = next_live_layer(plan.live, last_ordinal) {
                    break Some(live);
                }
                if plan.state != RecordingState::Live {
                    break None;
                }
                match layer_wake.next().await {
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        Err(io::Error::other(format!("recording layer subscription failed: {error}")))?;
                    }
                    None => Err(io::Error::other("recording layer subscription ended"))?,
                }
            };

            let Some(live) = next else { break; };
            let ordinal = live.descriptor.ordinal;
            let layer_id = live.descriptor.layer_id.clone();
            let message_start = if first_layer {
                match start {
                    LiveRrdStart::Bootstrap => LiveMessageStart::Bootstrap,
                    LiveRrdStart::ResumeHead => LiveMessageStart::ResumeHead,
                }
            } else {
                LiveMessageStart::ContinuedSegment
            };
            let mut receiver = stream_live_message_batches(
                live.path,
                recording_id,
                history,
                playback_store_id.clone(),
                message_start,
            );
            tracing::info!(
                %recording_id,
                %layer_id,
                ordinal,
                "governed Rerun channel following live layer"
            );
            while let Some(batch) = receiver.recv().await {
                let mut batch = batch?;
                if store_info_sent {
                    batch.retain(|message| !matches!(message, LogMsg::SetStoreInfo(_)));
                } else if batch
                    .iter()
                    .any(|message| matches!(message, LogMsg::SetStoreInfo(_)))
                {
                    store_info_sent = true;
                }
                if batch.is_empty() {
                    continue;
                }
                yield encode_frame(batch)?;
            }
            first_layer = false;
            last_ordinal = Some(ordinal);
        }
    };
    Box::pin(stream)
}

fn next_live_layer(
    live: Option<PlaybackLiveLayerPlan>,
    last_ordinal: Option<i64>,
) -> Option<PlaybackLiveLayerPlan> {
    live.filter(|candidate| last_ordinal.is_none_or(|last| candidate.descriptor.ordinal > last))
}

fn encode_frame(messages: Vec<LogMsg>) -> Result<Bytes, io::Error> {
    let mut encoder = Encoder::new_eager(
        CrateVersion::LOCAL,
        EncodingOptions::PROTOBUF_COMPRESSED,
        Vec::new(),
    )
    .context("opening live RRD frame encoder")
    .map_err(io::Error::other)?;
    for message in messages {
        encoder
            .append(&message)
            .context("encoding live RRD frame message")
            .map_err(io::Error::other)?;
    }
    encoder
        .finish()
        .context("finishing live RRD frame")
        .map_err(io::Error::other)?;
    let encoded = encoder
        .into_inner()
        .context("extracting live RRD frame")
        .map_err(io::Error::other)?;
    if encoded.len() > MAX_RRD_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "live RRD frame is {} bytes; maximum is {MAX_RRD_FRAME_BYTES}",
                encoded.len()
            ),
        ));
    }
    let frame_len = u32::try_from(encoded.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "live RRD frame exceeds u32"))?;
    let mut framed = BytesMut::with_capacity(4 + encoded.len());
    framed.put_u32(frame_len);
    framed.extend_from_slice(&encoded);
    Ok(framed.freeze())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use re_log_encoding::Decoder;
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;

    use super::*;

    #[test]
    fn start_header_has_two_exact_channel_states() {
        assert_eq!(
            LiveRrdStart::parse("bootstrap"),
            Some(LiveRrdStart::Bootstrap)
        );
        assert_eq!(
            LiveRrdStart::parse("resume-head"),
            Some(LiveRrdStart::ResumeHead)
        );
        assert_eq!(LiveRrdStart::parse("resume"), None);
        assert_eq!(LiveRrdStart::parse(""), None);
    }

    #[test]
    fn frame_contains_one_complete_rerun_rrd_payload() {
        let (recording, storage) = RecordingStreamBuilder::new("inspection-camera")
            .recording_id("source-recording")
            .memory()
            .unwrap();
        recording
            .log("sensor/value", &Scalars::single(42.0))
            .unwrap();
        let messages = storage.take();
        let expected = messages.len();

        let frame = encode_frame(messages).unwrap();
        let encoded_len = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
        assert_eq!(encoded_len, frame.len() - 4);
        let decoded = Decoder::<LogMsg>::decode_eager(Cursor::new(&frame[4..]))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(decoded.len(), expected);
    }

    #[test]
    fn rollover_advances_only_to_a_strictly_newer_writing_layer() {
        fn layer(ordinal: i64) -> PlaybackLiveLayerPlan {
            PlaybackLiveLayerPlan {
                descriptor: crate::contract::PlaybackLiveReceiver {
                    layer_id: uuid::Uuid::now_v7().to_string(),
                    layer_name: format!("capture-{ordinal:020}"),
                    ordinal,
                    current_byte_len: 0,
                    history_seconds: 1,
                    video_preroll_seconds: 2,
                    transport: crate::contract::PlaybackLiveTransport::RerunRrdChannelV2,
                },
                path: std::path::PathBuf::from("/tmp/recording.rrd"),
            }
        }

        assert_eq!(
            next_live_layer(Some(layer(2)), None)
                .unwrap()
                .descriptor
                .ordinal,
            2
        );
        assert!(next_live_layer(Some(layer(2)), Some(2)).is_none());
        assert!(next_live_layer(Some(layer(1)), Some(2)).is_none());
        assert_eq!(
            next_live_layer(Some(layer(3)), Some(2))
                .unwrap()
                .descriptor
                .ordinal,
            3
        );
    }
}
