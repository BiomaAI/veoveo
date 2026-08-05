use anyhow::{Context, Result, ensure};
use re_build_info::CrateVersion;
use re_log_encoding::{Encoder, EncodingOptions};
use re_log_types::{LogMsg, StoreId, StoreKind};
use sha2::{Digest, Sha256};
use veoveo_recording_protocol::v1::{RecordingBatch, RerunPayloadFormat};
use veoveo_rrd::video::{RrdVideoBoundary, inspect_log_message_video_boundary};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BatchBoundary {
    Continue,
    StartVideoSample,
    StartVideoGop,
    SourceSpan,
}

#[derive(Debug)]
pub struct RecordingAccumulator {
    store_id: StoreId,
    store_info: Option<LogMsg>,
    messages: Vec<LogMsg>,
    first_source_generation_ns: Option<u64>,
}

impl RecordingAccumulator {
    pub fn new(store_id: StoreId) -> Result<Self> {
        ensure!(
            store_id.kind() == StoreKind::Recording,
            "forwarder accepts recording stores only"
        );
        Ok(Self {
            store_id,
            store_info: None,
            messages: Vec::new(),
            first_source_generation_ns: None,
        })
    }

    pub fn store_id(&self) -> &StoreId {
        &self.store_id
    }

    pub fn push(&mut self, message: LogMsg) -> Result<()> {
        ensure!(
            message.store_id() == &self.store_id,
            "Rerun message changed store identity"
        );
        if matches!(message, LogMsg::SetStoreInfo(_)) {
            ensure!(
                self.messages.is_empty(),
                "store information changed while a batch was pending"
            );
            self.store_info = Some(message);
        } else {
            ensure!(
                self.store_info.is_some(),
                "Rerun data arrived before SetStoreInfo"
            );
            if self.first_source_generation_ns.is_none() {
                self.first_source_generation_ns = source_generation_ns(&message);
            }
            self.messages.push(message);
        }
        Ok(())
    }

    pub fn boundary_before(
        &self,
        message: &LogMsg,
        maximum_source_span: std::time::Duration,
    ) -> Result<BatchBoundary> {
        if self.messages.is_empty() {
            return Ok(BatchBoundary::Continue);
        }
        let mut video = RrdVideoBoundary::default();
        inspect_log_message_video_boundary(message, &mut video)?;
        if video.contains_video {
            return Ok(if video.begins_with_keyframe {
                BatchBoundary::StartVideoGop
            } else {
                BatchBoundary::StartVideoSample
            });
        }
        let maximum_source_span_ns = u64::try_from(maximum_source_span.as_nanos())?;
        Ok(
            match (
                self.first_source_generation_ns,
                source_generation_ns(message),
            ) {
                (Some(first), Some(next))
                    if next.saturating_sub(first) >= maximum_source_span_ns =>
                {
                    BatchBoundary::SourceSpan
                }
                _ => BatchBoundary::Continue,
            },
        )
    }

    pub fn pending_len(&self) -> usize {
        self.messages.len()
    }

    pub fn drain_encoded(&mut self, maximum_batch_bytes: u64) -> Result<Vec<RecordingBatch>> {
        if self.messages.is_empty() {
            return Ok(Vec::new());
        }
        let store_info = self
            .store_info
            .as_ref()
            .context("Rerun batch has no SetStoreInfo")?;
        let messages = std::mem::take(&mut self.messages);
        self.first_source_generation_ns = None;
        encode_split(store_info, &messages, maximum_batch_bytes)
    }
}

fn source_generation_ns(message: &LogMsg) -> Option<u64> {
    match message {
        LogMsg::ArrowMsg(_, message) => Some(message.chunk_id.nanos_since_epoch()),
        LogMsg::SetStoreInfo(_) | LogMsg::BlueprintActivationCommand(_) => None,
    }
}

fn encode_split(
    store_info: &LogMsg,
    messages: &[LogMsg],
    maximum_batch_bytes: u64,
) -> Result<Vec<RecordingBatch>> {
    let encoded_rrd = encode_rrd(store_info, messages)?;
    if encoded_rrd.len() as u64 <= maximum_batch_bytes {
        return Ok(vec![RecordingBatch {
            sequence: 0,
            payload_format: RerunPayloadFormat::Rrd0350.into(),
            sha256: Sha256::digest(&encoded_rrd).to_vec(),
            encoded_rrd,
            message_count: u64::try_from(messages.len() + 1)?,
        }]);
    }
    ensure!(
        messages.len() > 1,
        "one Rerun message exceeds the configured batch byte limit"
    );
    let middle = messages.len() / 2;
    let mut batches = encode_split(store_info, &messages[..middle], maximum_batch_bytes)?;
    batches.extend(encode_split(
        store_info,
        &messages[middle..],
        maximum_batch_bytes,
    )?);
    Ok(batches)
}

fn encode_rrd(store_info: &LogMsg, messages: &[LogMsg]) -> Result<Vec<u8>> {
    let mut encoder = Encoder::new_eager(
        CrateVersion::LOCAL,
        EncodingOptions::PROTOBUF_COMPRESSED,
        Vec::new(),
    )
    .context("opening Rerun RRD batch encoder")?;
    encoder
        .append(store_info)
        .context("encoding Rerun store information")?;
    for message in messages {
        encoder
            .append(message)
            .context("encoding Rerun batch message")?;
    }
    encoder.finish().context("finishing Rerun RRD batch")?;
    encoder
        .into_inner()
        .context("extracting encoded Rerun batch")
}

#[cfg(test)]
mod tests {
    use std::io::{BufReader, Cursor};

    use re_log_encoding::Decoder;
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::{
        archetypes::{Scalars, VideoStream},
        components::VideoCodec,
    };

    use super::*;

    #[test]
    fn emits_a_complete_decodable_rrd_batch() {
        let (recording, storage) = RecordingStreamBuilder::new("inspection-camera")
            .recording_id("run-a")
            .memory()
            .unwrap();
        recording
            .log("sensor/value", &Scalars::single(42.0))
            .unwrap();
        let messages = storage.take();
        let store_id = messages[0].store_id().clone();
        let mut accumulator = RecordingAccumulator::new(store_id).unwrap();
        for message in messages {
            if matches!(message, LogMsg::SetStoreInfo(_)) && accumulator.pending_len() > 0 {
                break;
            }
            accumulator.push(message).unwrap();
        }

        let mut batches = accumulator.drain_encoded(8 * 1024 * 1024).unwrap();
        assert_eq!(batches.len(), 1);
        batches[0].sequence = 1;
        batches[0].validate(8 * 1024 * 1024).unwrap();
        let decoded =
            Decoder::<LogMsg>::decode_eager(BufReader::new(Cursor::new(&batches[0].encoded_rrd)))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
        assert_eq!(decoded.len() as u64, batches[0].message_count);
        assert!(decoded.iter().all(|message| {
            message.store_id().application_id().as_str() == "inspection-camera"
                && message.store_id().recording_id().as_str() == "run-a"
        }));
    }

    #[test]
    fn starts_a_new_batch_at_each_video_sample() {
        let (recording, storage) = RecordingStreamBuilder::new("inspection-camera")
            .recording_id("run-a")
            .memory()
            .unwrap();
        recording
            .log("sensor/value", &Scalars::single(42.0))
            .unwrap();
        recording
            .log(
                "camera/front",
                &VideoStream::new(VideoCodec::H264).with_sample(vec![0, 0, 0, 1, 0x65, 1]),
            )
            .unwrap();
        let messages = storage.take();
        let store_id = messages[0].store_id().clone();
        let mut accumulator = RecordingAccumulator::new(store_id).unwrap();
        let mut batches = Vec::new();
        for message in messages {
            if accumulator
                .boundary_before(&message, std::time::Duration::from_millis(750))
                .unwrap()
                != BatchBoundary::Continue
            {
                batches.extend(accumulator.drain_encoded(8 * 1024 * 1024).unwrap());
            }
            accumulator.push(message).unwrap();
        }
        batches.extend(accumulator.drain_encoded(8 * 1024 * 1024).unwrap());

        assert_eq!(batches.len(), 2);
        assert!(
            !veoveo_rrd::video::inspect_rrd_video_boundary(&batches[0].encoded_rrd)
                .unwrap()
                .contains_video
        );
        let video = veoveo_rrd::video::inspect_rrd_video_boundary(&batches[1].encoded_rrd).unwrap();
        assert!(video.contains_video);
        assert!(video.begins_with_keyframe);
    }

    #[test]
    fn closes_a_batch_from_source_generation_progress_without_a_timer() {
        let (recording, storage) = RecordingStreamBuilder::new("inspection-telemetry")
            .recording_id("run-a")
            .memory()
            .unwrap();
        recording
            .log("sensor/value", &Scalars::single(1.0))
            .unwrap();
        recording.flush_blocking().unwrap();
        recording
            .log("sensor/value", &Scalars::single(2.0))
            .unwrap();
        let mut messages = storage.take();
        let mut arrow_messages = messages
            .iter_mut()
            .filter_map(|message| match message {
                LogMsg::ArrowMsg(_, message) => Some(message),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(arrow_messages.len(), 2);
        arrow_messages[0].chunk_id = re_tuid::Tuid::from_nanos_and_inc(1_000_000_000, 1);
        arrow_messages[1].chunk_id = re_tuid::Tuid::from_nanos_and_inc(1_750_000_000, 2);

        drop(arrow_messages);
        let store_info = messages
            .iter()
            .find(|message| matches!(message, LogMsg::SetStoreInfo(_)))
            .unwrap()
            .clone();
        let arrow_messages = messages
            .iter()
            .filter(|message| matches!(message, LogMsg::ArrowMsg(_, _)))
            .cloned()
            .collect::<Vec<_>>();
        let store_id = store_info.store_id().clone();
        let mut accumulator = RecordingAccumulator::new(store_id).unwrap();
        accumulator.push(store_info).unwrap();
        accumulator.push(arrow_messages[0].clone()).unwrap();
        assert_eq!(
            accumulator
                .boundary_before(&arrow_messages[1], std::time::Duration::from_millis(750),)
                .unwrap(),
            BatchBoundary::SourceSpan
        );
    }
}
