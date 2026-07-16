use anyhow::{Context, Result, ensure};
use re_build_info::CrateVersion;
use re_log_encoding::{Encoder, EncodingOptions};
use re_log_types::{LogMsg, StoreId, StoreKind};
use sha2::{Digest, Sha256};
use veoveo_recording_protocol::v1::{RecordingBatch, RerunPayloadFormat};

#[derive(Debug)]
pub struct RecordingAccumulator {
    store_id: StoreId,
    store_info: Option<LogMsg>,
    messages: Vec<LogMsg>,
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
            self.messages.push(message);
        }
        Ok(())
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
        encode_split(store_info, &messages, maximum_batch_bytes)
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
            payload_format: RerunPayloadFormat::Rrd0341.into(),
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
    use re_sdk_types::archetypes::Scalars;

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
}
