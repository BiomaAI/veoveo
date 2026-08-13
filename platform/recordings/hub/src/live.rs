//! Rerun-native projections for bounded live playback.

use std::{collections::BTreeMap, sync::Arc};

use anyhow::{Context, Result, ensure};
use re_chunk_store::{CompactionOptions, IsStartOfGop, OptimizationProfile};
use re_entity_db::EntityDb;
use re_log_types::{LogMsg, StoreId};
use re_sdk_types::archetypes::VideoStream;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveRrdBatchKind {
    Bootstrap,
    Incremental,
}

/// Compact one bounded live message batch with Rerun's live-viewer profile.
///
/// The input remains the authoritative durable batch sequence. This projection
/// removes repeated store metadata and merges its many one-row SDK chunks
/// before a browser has to index them on its rendering thread.
pub fn optimize_live_rrd_messages(
    messages: Vec<LogMsg>,
    kind: LiveRrdBatchKind,
) -> Result<Vec<LogMsg>> {
    ensure!(
        !messages.is_empty(),
        "live RRD optimization requires at least one message"
    );
    let profile = OptimizationProfile::LIVE;
    let mut store_config = profile.to_chunk_store_config();
    store_config.enable_changelog = false;
    let mut stores = BTreeMap::<StoreId, EntityDb>::new();
    for message in &messages {
        let database = stores.entry(message.store_id().clone()).or_insert_with(|| {
            EntityDb::with_store_config(message.store_id().clone(), false, store_config.clone())
        });
        database
            .add_log_msg(message)
            .context("indexing bounded live RRD batch")?;
    }
    ensure!(
        stores.len() == 1,
        "bounded live RRD batch must contain exactly one store"
    );

    let is_start_of_gop: IsStartOfGop = Arc::new(|data, codec| {
        ensure!(
            codec == re_sdk_types::components::VideoCodec::H264,
            "live RRD optimization supports H.264 VideoStream data"
        );
        crate::h264_access_unit_is_decoder_reentrant(data)
    });
    let options = CompactionOptions {
        config: store_config,
        num_extra_passes: Some(profile.num_extra_passes as usize),
        is_start_of_gop: (kind == LiveRrdBatchKind::Bootstrap).then_some(is_start_of_gop),
        split_size_ratio: profile.split_size_ratio,
        fix_keyframe: false,
    };
    for database in stores.values() {
        // Safety: this optimizer exclusively owns each headless EntityDb.
        #[expect(unsafe_code)]
        let engine = unsafe { database.storage_engine_raw() };
        let compacted = engine
            .read()
            .store()
            .compacted(&options)
            .context("compacting bounded live RRD batch")?;
        *engine.write().store() = compacted;
    }

    let messages = stores
        .values()
        .flat_map(|database| database.to_messages(None))
        .collect::<Result<Vec<_>, _>>()
        .context("encoding bounded live RRD batch messages")?;
    omit_live_video_keyframe_markers(messages)
}

/// Remove producer-authored and compaction-authored keyframe columns from the
/// browser live projection.
///
/// Rerun 0.35 discovers H.264 sync samples from the encoded access units. Its
/// viewer cache also assumes every `VideoStream:sample` value is dense within
/// the physical chunk. A sparse keyframe marker can otherwise become the
/// compaction key that co-locates samples from different batches and violates
/// that invariant. The durable source remains unchanged, and archive
/// materialization derives canonical keyframe markers from the H.264 bytes.
fn omit_live_video_keyframe_markers(messages: Vec<LogMsg>) -> Result<Vec<LogMsg>> {
    let keyframe_component = VideoStream::descriptor_is_keyframe().component;
    let mut projected = Vec::with_capacity(messages.len());
    for message in messages {
        let LogMsg::ArrowMsg(store_id, arrow) = message else {
            projected.push(message);
            continue;
        };
        let chunk =
            re_chunk::Chunk::from_arrow_msg(&arrow).context("decoding bounded live RRD chunk")?;
        if chunk.raw_component_array(keyframe_component).is_none() {
            projected.push(LogMsg::ArrowMsg(store_id, arrow));
            continue;
        }
        let chunk = chunk.component_dropped(keyframe_component);
        if chunk.num_components() == 0 {
            continue;
        }
        projected.push(LogMsg::ArrowMsg(
            store_id,
            chunk
                .to_arrow_msg()
                .context("encoding bounded live RRD chunk without keyframe markers")?,
        ));
    }
    Ok(projected)
}

#[cfg(test)]
mod tests {
    use re_log_types::LogMsg;
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::{
        archetypes::{Scalars, VideoStream},
        components::{IsKeyframe, VideoCodec},
    };

    use super::*;

    #[test]
    fn live_profile_compacts_one_row_sdk_chunks_without_losing_rows() {
        let (recording, storage) = RecordingStreamBuilder::new("live-test")
            .recording_id("recording-a")
            .memory()
            .unwrap();
        let mut messages = Vec::new();
        for value in 0..32 {
            recording
                .log("sensor/value", &Scalars::single(value as f64))
                .unwrap();
            recording.flush_blocking().unwrap();
            messages.extend(storage.take());
        }
        let input_chunks = messages
            .iter()
            .filter(|message| matches!(message, LogMsg::ArrowMsg(_, _)))
            .count();
        assert_eq!(input_chunks, 32);

        let optimized =
            optimize_live_rrd_messages(messages, LiveRrdBatchKind::Incremental).unwrap();
        let chunks = optimized
            .iter()
            .filter_map(|message| match message {
                LogMsg::ArrowMsg(_, arrow) => Some(re_chunk::Chunk::from_arrow_msg(arrow).unwrap()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(chunks.len() < input_chunks);
        assert_eq!(
            chunks.iter().map(re_chunk::Chunk::num_rows).sum::<usize>(),
            32
        );
    }

    #[test]
    fn live_projection_omits_keyframe_markers_and_keeps_samples_dense() {
        const KEYFRAME: &[u8] = &[
            0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 1, 0, 0, 0, 1, 0x65, 1,
        ];
        const INTER_FRAME: &[u8] = &[0, 0, 0, 1, 0x41, 1];

        let (recording, storage) = RecordingStreamBuilder::new("live-video-test")
            .recording_id("recording-video")
            .memory()
            .unwrap();
        recording
            .log_static("camera/front", &VideoStream::new(VideoCodec::H264))
            .unwrap();
        for index in 0..4 {
            recording.set_time_sequence("frame", index);
            let sample = if index == 0 { KEYFRAME } else { INTER_FRAME };
            let mut stream = VideoStream::update_fields().with_sample(sample.to_vec());
            if index == 0 {
                stream = stream.with_is_keyframe(IsKeyframe::from(true));
            }
            recording.log("camera/front", &stream).unwrap();
            recording.flush_blocking().unwrap();
        }

        let optimized =
            optimize_live_rrd_messages(storage.take(), LiveRrdBatchKind::Bootstrap).unwrap();
        let keyframe_component = VideoStream::descriptor_is_keyframe().component;
        let sample_component = VideoStream::descriptor_sample().component;
        let mut sample_rows = 0;
        for message in &optimized {
            let LogMsg::ArrowMsg(_, arrow) = message else {
                continue;
            };
            let chunk = re_chunk::Chunk::from_arrow_msg(arrow).unwrap();
            assert!(
                chunk.raw_component_array(keyframe_component).is_none(),
                "live projection must not expose sparse keyframe markers"
            );
            if chunk.raw_component_array(sample_component).is_some() {
                let offsets = chunk
                    .iter_component_offsets(sample_component)
                    .collect::<Vec<_>>();
                assert_eq!(offsets.len(), chunk.num_rows());
                assert!(offsets.iter().all(|offset| offset.len == 1));
                sample_rows += chunk.num_rows();
            }
        }
        assert_eq!(sample_rows, 4);
    }
}
