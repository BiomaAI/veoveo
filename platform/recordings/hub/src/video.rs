//! Typed extraction of encoded video samples from one logical Rerun recording.
//!
//! The recording hub rotates a logical recording across multiple physical RRD
//! files. Video readers must therefore load the authorized segment set as one
//! store before selecting samples: codec state and the keyframe immediately
//! preceding a requested range may live in an earlier segment.

use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use h264_reader::nal::sps::SeqParameterSet;
use h264_reader::nal::{Nal as _, RefNal};
use mp4::{AvcConfig, MediaConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig, TrackType};
use re_chunk_store::{ChunkStore, ChunkStoreConfig, ChunkStoreHandle};
use re_dataframe::{
    AbsoluteTimeRange, EntityPath, QueryEngine, QueryExpression, SparseFillStrategy, TimeInt,
};
use re_log_encoding::rrd::Decoder;
use re_log_types::{LogMsg, TimeType};
use re_sdk_types::archetypes::VideoStream;
use re_sdk_types::components::{IsKeyframe, VideoCodec, VideoSample};
use re_sdk_types::external::arrow::array::{Array as _, ListArray};
use re_sdk_types::external::re_types_core::Loggable;

/// Veoveo's canonical encoded-video ingest profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum H264VideoProfile {
    /// H.264 access units in Annex B form, decoder-reentrant IDRs, and no B-frames.
    AnnexBNoBFrames,
}

/// Return whether an Annex B access unit contains an H.264 IDR picture.
pub fn h264_access_unit_is_idr(bytes: &[u8]) -> Result<bool> {
    Ok(annex_b_nals(bytes)?.iter().any(|nal| nal[0] & 0x1f == 5))
}

/// How values on the selected Rerun index should be interpreted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoIndexKind {
    Sequence,
    DurationNanoseconds,
    TimestampNanoseconds,
}

impl From<TimeType> for VideoIndexKind {
    fn from(value: TimeType) -> Self {
        match value {
            TimeType::Sequence => Self::Sequence,
            TimeType::DurationNs => Self::DurationNanoseconds,
            TimeType::TimestampNs => Self::TimestampNanoseconds,
        }
    }
}

/// A bounded request for encoded samples from one video entity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoClipRequest {
    pub application_id: String,
    pub recording_key: String,
    pub entity_path: String,
    pub timeline: String,
    pub start_index: i64,
    pub end_index: i64,
    pub max_samples: usize,
    pub max_encoded_bytes: u64,
}

impl VideoClipRequest {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.application_id.is_empty(),
            "application_id is required"
        );
        ensure!(!self.recording_key.is_empty(), "recording_key is required");
        ensure!(!self.entity_path.is_empty(), "entity_path is required");
        ensure!(!self.timeline.is_empty(), "timeline is required");
        ensure!(
            self.start_index <= self.end_index,
            "start_index must not exceed end_index"
        );
        ensure!(self.max_samples > 0, "max_samples must be non-zero");
        ensure!(
            self.max_encoded_bytes > 0,
            "max_encoded_bytes must be non-zero"
        );
        Ok(())
    }
}

/// One decoder input access unit and its original Rerun index.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedVideoSample {
    pub index: i64,
    pub is_keyframe: bool,
    pub bytes: Vec<u8>,
}

/// A selected video range, including any keyframe preroll required to decode it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedVideoClip {
    pub profile: H264VideoProfile,
    pub index_kind: VideoIndexKind,
    pub application_id: String,
    pub recording_key: String,
    pub entity_path: String,
    pub timeline: String,
    pub requested_start_index: i64,
    pub requested_end_index: i64,
    pub decode_start_index: i64,
    pub width: u16,
    pub height: u16,
    pub encoded_bytes: u64,
    pub samples: Vec<EncodedVideoSample>,
}

/// Extract a clip from an explicit, already-authorized set of RRD segments.
pub fn extract_video_clip(
    segments: &[PathBuf],
    request: &VideoClipRequest,
) -> Result<EncodedVideoClip> {
    ensure!(!segments.is_empty(), "recording has no readable segments");
    let mut messages = Vec::new();
    for segment in segments {
        let file = File::open(segment)
            .with_context(|| format!("opening video segment {}", segment.display()))?;
        let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(file))
            .with_context(|| format!("decoding video segment {}", segment.display()))?;
        for message in decoder {
            messages.push(
                message.with_context(|| format!("reading video segment {}", segment.display()))?,
            );
        }
    }
    extract_video_clip_from_messages(messages, request)
}

/// Extract a clip from Rerun messages, used by both durable segments and the
/// embedded proxy's recent replay stream.
pub fn extract_video_clip_from_messages(
    messages: impl IntoIterator<Item = LogMsg>,
    request: &VideoClipRequest,
) -> Result<EncodedVideoClip> {
    request.validate()?;
    let stores = ChunkStore::from_log_msgs(&ChunkStoreConfig::DEFAULT, messages)
        .context("building recording store from Rerun messages")?;
    let (store_id, store) = stores
        .into_iter()
        .find(|(store_id, _)| {
            store_id.is_recording()
                && store_id.application_id().as_str() == request.application_id
                && store_id.recording_id().as_str() == request.recording_key
        })
        .context("authorized segment set does not contain the requested Rerun recording")?;
    ensure!(
        store_id.is_recording(),
        "selected Rerun store is not a recording"
    );

    let engine = QueryEngine::from_store(ChunkStoreHandle::new(store));
    let schema = engine.schema();
    let timeline = schema
        .indices
        .iter()
        .find(|descriptor| descriptor.timeline_name().as_str() == request.timeline)
        .with_context(|| format!("recording has no timeline `{}`", request.timeline))?
        .timeline();
    let index_kind = VideoIndexKind::from(timeline.typ());
    let entity_path = EntityPath::from(request.entity_path.as_str());
    let query = QueryExpression {
        view_contents: Some([(entity_path, None)].into_iter().collect()),
        filtered_index: Some(*timeline.name()),
        filtered_index_range: Some(AbsoluteTimeRange::new(
            TimeInt::MIN,
            TimeInt::new_temporal(request.end_index),
        )),
        sparse_fill_strategy: SparseFillStrategy::None,
        ..Default::default()
    };
    let mut handle = engine.query(query);
    let result_schema = handle.schema().clone();
    let timeline_column = column_index(&result_schema, request.timeline.as_str())?;
    let sample_column = column_index(
        &result_schema,
        &component_column(&request.entity_path, &VideoStream::descriptor_sample()),
    )?;
    let codec_column = column_index(
        &result_schema,
        &component_column(&request.entity_path, &VideoStream::descriptor_codec()),
    )?;
    let keyframe_column = column_index(
        &result_schema,
        &component_column(&request.entity_path, &VideoStream::descriptor_is_keyframe()),
    )?;

    let mut samples = Vec::new();
    for batch in handle.batch_iter() {
        let time_array = batch.column(timeline_column);
        let (_, time_values) = TimeType::from_arrow_array(time_array.as_ref())
            .context("decoding video timeline values")?;
        for row in 0..batch.num_rows() {
            if time_array.is_null(row) {
                continue;
            }
            let Some(sample) = component_at::<VideoSample>(batch.column(sample_column), row)?
            else {
                continue;
            };
            let codec = component_at::<VideoCodec>(batch.column(codec_column), row)?
                .context("video sample is missing codec metadata")?;
            ensure!(
                codec == VideoCodec::H264,
                "only H.264 VideoStream samples are supported"
            );
            let is_keyframe =
                component_at::<IsKeyframe>(batch.column(keyframe_column), row)?.is_some();
            samples.push(EncodedVideoSample {
                index: time_values[row],
                is_keyframe,
                bytes: sample.0.0.as_ref().to_vec(),
            });
        }
    }
    samples.sort_by_key(|sample| sample.index);
    for pair in samples.windows(2) {
        ensure!(
            pair[0].index != pair[1].index,
            "video entity contains more than one sample at index {}",
            pair[0].index
        );
    }
    let first_requested = samples
        .iter()
        .position(|sample| sample.index >= request.start_index)
        .context("requested range contains no video samples")?;
    let decode_start = samples[..=first_requested]
        .iter()
        .rposition(|sample| sample.is_keyframe)
        .context("no decoder-reentrant keyframe exists at or before the requested range")?;
    let mut selected = samples
        .into_iter()
        .skip(decode_start)
        .take_while(|sample| sample.index <= request.end_index)
        .collect::<Vec<_>>();
    ensure!(
        !selected.is_empty(),
        "requested range contains no video samples"
    );
    ensure!(
        selected[0].is_keyframe,
        "selected video clip does not begin at a keyframe"
    );
    ensure!(
        annex_b_nals(&selected[0].bytes)?
            .iter()
            .any(|nal| nal[0] & 0x1f == 5),
        "selected keyframe is not a decoder-reentrant H.264 IDR"
    );
    ensure!(
        selected.len() <= request.max_samples,
        "selected video clip exceeds max_samples ({})",
        request.max_samples
    );
    let encoded_bytes = selected.iter().try_fold(0_u64, |total, sample| {
        let len = u64::try_from(sample.bytes.len()).context("video sample exceeds u64")?;
        total.checked_add(len).context("video byte count overflow")
    })?;
    ensure!(
        encoded_bytes <= request.max_encoded_bytes,
        "selected video clip exceeds max_encoded_bytes ({})",
        request.max_encoded_bytes
    );
    let decode_start_index = selected[0].index;
    let (_, _, width, height) = h264_parameter_sets(&selected[..1])?;
    // Drop samples before the requested range only when they precede the
    // decoder-reentrant keyframe. The retained prefix is required preroll.
    selected.shrink_to_fit();
    Ok(EncodedVideoClip {
        profile: H264VideoProfile::AnnexBNoBFrames,
        index_kind,
        application_id: request.application_id.clone(),
        recording_key: request.recording_key.clone(),
        entity_path: request.entity_path.clone(),
        timeline: request.timeline.clone(),
        requested_start_index: request.start_index,
        requested_end_index: request.end_index,
        decode_start_index,
        width,
        height,
        encoded_bytes,
        samples: selected,
    })
}

/// Remux an H.264 clip to MP4 without decoding or re-encoding it.
pub fn remux_h264_mp4(clip: &EncodedVideoClip) -> Result<Vec<u8>> {
    ensure!(
        matches!(
            clip.index_kind,
            VideoIndexKind::DurationNanoseconds | VideoIndexKind::TimestampNanoseconds
        ),
        "MP4 remux requires a duration or timestamp timeline"
    );
    ensure!(!clip.samples.is_empty(), "video clip has no samples");
    let (sps, pps, width, height) = h264_parameter_sets(&clip.samples[..1])?;
    let config = Mp4Config {
        major_brand: "isom".parse()?,
        minor_version: 512,
        compatible_brands: vec![
            "isom".parse()?,
            "iso2".parse()?,
            "avc1".parse()?,
            "mp41".parse()?,
        ],
        timescale: 1_000,
    };
    let cursor = Cursor::new(Vec::new());
    let mut writer = Mp4Writer::write_start(cursor, &config)?;
    writer.add_track(&TrackConfig {
        track_type: TrackType::Video,
        timescale: 1_000_000_000,
        language: "und".to_owned(),
        media_conf: MediaConfig::AvcConfig(AvcConfig {
            width,
            height,
            seq_param_set: sps,
            pic_param_set: pps,
        }),
    })?;

    let fallback_duration = clip
        .samples
        .windows(2)
        .map(|pair| pair[1].index - pair[0].index)
        .find(|duration| *duration > 0)
        .unwrap_or(33_333_333);
    for (index, sample) in clip.samples.iter().enumerate() {
        let duration = clip
            .samples
            .get(index + 1)
            .map_or(fallback_duration, |next| next.index - sample.index);
        ensure!(duration > 0, "video sample timestamps must increase");
        writer.write_sample(
            1,
            &Mp4Sample {
                start_time: u64::try_from(sample.index - clip.decode_start_index)
                    .context("video sample precedes decode start")?,
                duration: u32::try_from(duration).context("video sample duration exceeds u32")?,
                rendering_offset: 0,
                is_sync: sample.is_keyframe,
                bytes: annex_b_to_avcc(&sample.bytes)?.into(),
            },
        )?;
    }
    writer.write_end()?;
    Ok(writer.into_writer().into_inner())
}

fn component_column(entity_path: &str, descriptor: &re_sdk_types::ComponentDescriptor) -> String {
    format!("{entity_path}:{}", descriptor.component)
}

fn column_index(
    schema: &re_sdk_types::external::arrow::datatypes::Schema,
    name: &str,
) -> Result<usize> {
    schema
        .fields()
        .iter()
        .position(|field| field.name() == name)
        .with_context(|| format!("video query did not return column `{name}`"))
}

fn component_at<T: Loggable>(
    column: &dyn re_sdk_types::external::arrow::array::Array,
    row: usize,
) -> Result<Option<T>> {
    let rows = column
        .as_any()
        .downcast_ref::<ListArray>()
        .context("Rerun component column is not a list array")?;
    if rows.is_null(row) || rows.value_length(row) == 0 {
        return Ok(None);
    }
    let values =
        T::from_arrow(rows.value(row).as_ref()).context("deserializing Rerun video component")?;
    ensure!(
        values.len() == 1,
        "video sample row must contain exactly one component instance"
    );
    Ok(values.into_iter().next())
}

fn h264_parameter_sets(samples: &[EncodedVideoSample]) -> Result<(Vec<u8>, Vec<u8>, u16, u16)> {
    let mut sps = None;
    let mut pps = None;
    for sample in samples {
        for nal in annex_b_nals(&sample.bytes)? {
            match nal[0] & 0x1f {
                7 if sps.is_none() => sps = Some(nal.to_vec()),
                8 if pps.is_none() => pps = Some(nal.to_vec()),
                _ => {}
            }
        }
        if sps.is_some() && pps.is_some() {
            break;
        }
    }
    let sps = sps.context("H.264 clip has no SPS")?;
    let pps = pps.context("H.264 clip has no PPS")?;
    let parsed = SeqParameterSet::from_bits(RefNal::new(&sps, &[], true).rbsp_bits())
        .map_err(|error| anyhow::anyhow!("invalid H.264 SPS: {error:?}"))?;
    let (width, height) = parsed
        .pixel_dimensions()
        .map_err(|error| anyhow::anyhow!("invalid H.264 dimensions: {error:?}"))?;
    Ok((
        sps,
        pps,
        u16::try_from(width).context("H.264 width exceeds u16")?,
        u16::try_from(height).context("H.264 height exceeds u16")?,
    ))
}

fn annex_b_to_avcc(sample: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::with_capacity(sample.len());
    let mut has_picture = false;
    for nal in annex_b_nals(sample)? {
        let unit_type = nal[0] & 0x1f;
        if matches!(unit_type, 7..=9) {
            continue;
        }
        has_picture |= matches!(unit_type, 1 | 5);
        output.extend_from_slice(
            &u32::try_from(nal.len())
                .context("H.264 NAL exceeds u32")?
                .to_be_bytes(),
        );
        output.extend_from_slice(nal);
    }
    ensure!(has_picture, "H.264 sample has no coded picture NAL");
    Ok(output)
}

fn annex_b_nals(bytes: &[u8]) -> Result<Vec<&[u8]>> {
    let mut starts = Vec::new();
    let mut cursor = 0;
    while cursor + 3 <= bytes.len() {
        let start_code = if bytes[cursor..].starts_with(&[0, 0, 0, 1]) {
            Some(4)
        } else if bytes[cursor..].starts_with(&[0, 0, 1]) {
            Some(3)
        } else {
            None
        };
        if let Some(length) = start_code {
            starts.push((cursor, cursor + length));
            cursor += length;
        } else {
            cursor += 1;
        }
    }
    ensure!(!starts.is_empty(), "H.264 sample is not Annex B");
    let mut nals = Vec::with_capacity(starts.len());
    for (index, (_, payload_start)) in starts.iter().copied().enumerate() {
        let payload_end = starts
            .get(index + 1)
            .map_or(bytes.len(), |(start, _)| *start);
        let mut payload_end = payload_end;
        while payload_end > payload_start && bytes[payload_end - 1] == 0 {
            payload_end -= 1;
        }
        ensure!(payload_start < payload_end, "H.264 contains an empty NAL");
        nals.push(&bytes[payload_start..payload_end]);
    }
    Ok(nals)
}
