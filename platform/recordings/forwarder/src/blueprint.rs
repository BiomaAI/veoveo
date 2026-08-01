//! Atomic collection of producer-authored Rerun Blueprint stores.

use anyhow::{Context, Result, ensure};
use re_build_info::CrateVersion;
use re_byte_size::SizeBytes as _;
use re_log_encoding::{Encoder, EncodingOptions};
use re_log_types::{LogMsg, StoreId, StoreKind};
use sha2::{Digest, Sha256};
use veoveo_recording_protocol::v1::{RecordingBlueprint, RerunPayloadFormat};

#[derive(Debug)]
pub struct BlueprintAccumulator {
    store_id: StoreId,
    messages: Vec<LogMsg>,
    activated: bool,
    store_info_count: u64,
    retained_bytes: u64,
    maximum_bytes: u64,
    maximum_messages: u64,
}

impl BlueprintAccumulator {
    pub fn new(store_id: StoreId, maximum_bytes: u64, maximum_messages: u64) -> Result<Self> {
        ensure!(
            store_id.kind() == StoreKind::Blueprint,
            "Blueprint accumulator requires a Blueprint store"
        );
        ensure!(
            maximum_bytes > 0 && maximum_messages > 0,
            "producer Blueprint publication is not enabled"
        );
        Ok(Self {
            store_id,
            messages: Vec::new(),
            activated: false,
            store_info_count: 0,
            retained_bytes: 0,
            maximum_bytes,
            maximum_messages,
        })
    }

    pub fn store_id(&self) -> &StoreId {
        &self.store_id
    }

    pub fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }

    pub fn push(&mut self, message: LogMsg) -> Result<bool> {
        ensure!(!self.activated, "Blueprint received data after activation");
        ensure!(
            message.store_id() == &self.store_id,
            "Blueprint message changed store identity"
        );
        ensure!(
            (self.messages.len() as u64) < self.maximum_messages,
            "Blueprint exceeds the advertised message-count limit"
        );
        let message_bytes = message.total_size_bytes();
        ensure!(
            self.retained_bytes.saturating_add(message_bytes) <= self.maximum_bytes,
            "Blueprint exceeds the advertised in-memory byte limit"
        );
        if matches!(message, LogMsg::SetStoreInfo(_)) {
            self.store_info_count += 1;
        }
        if let LogMsg::BlueprintActivationCommand(command) = &message {
            ensure!(
                command.make_active && command.make_default,
                "governed producer Blueprint must be active and default"
            );
            self.activated = true;
        }
        self.retained_bytes = self.retained_bytes.saturating_add(message_bytes);
        self.messages.push(message);
        Ok(self.activated)
    }

    pub fn finish(self) -> Result<RecordingBlueprint> {
        ensure!(self.activated, "Blueprint is incomplete");
        ensure!(self.store_info_count > 0, "Blueprint requires SetStoreInfo");
        let message_count = u64::try_from(self.messages.len())?;
        let mut encoder = Encoder::new_eager(
            CrateVersion::LOCAL,
            EncodingOptions::PROTOBUF_COMPRESSED,
            Vec::new(),
        )
        .context("opening Blueprint RRD encoder")?;
        for message in &self.messages {
            encoder
                .append(message)
                .context("encoding Blueprint message")?;
        }
        encoder.finish().context("finishing Blueprint RRD")?;
        let encoded_rrd = encoder.into_inner().context("extracting Blueprint RRD")?;
        ensure!(
            encoded_rrd.len() as u64 <= self.maximum_bytes,
            "Blueprint exceeds the advertised {}-byte limit",
            self.maximum_bytes
        );
        Ok(RecordingBlueprint {
            revision: 0,
            payload_format: RerunPayloadFormat::Rrd0350.into(),
            sha256: Sha256::digest(&encoded_rrd).to_vec(),
            encoded_rrd,
            message_count,
        })
    }
}

pub fn associated_recording<'a>(
    blueprint: &StoreId,
    recordings: impl Iterator<Item = &'a StoreId>,
) -> Result<&'a StoreId> {
    let matches = recordings
        .filter(|recording| {
            recording.kind() == StoreKind::Recording
                && recording.application_id() == blueprint.application_id()
        })
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "Blueprint application must have exactly one active recording in this forwarder; found {}",
        matches.len()
    );
    Ok(matches[0])
}

#[cfg(test)]
mod tests {
    use re_log_types::ApplicationId;

    use super::*;

    #[test]
    fn association_rejects_zero_or_ambiguous_recordings() {
        let blueprint = StoreId::default_blueprint(ApplicationId::from("anonymous-app"));
        let recording_a = StoreId::recording("anonymous-app", "a");
        let recording_b = StoreId::recording("anonymous-app", "b");
        assert!(associated_recording(&blueprint, std::iter::empty()).is_err());
        assert_eq!(
            associated_recording(&blueprint, [&recording_a].into_iter()).unwrap(),
            &recording_a
        );
        assert!(
            associated_recording(&blueprint, [&recording_a, &recording_b].into_iter()).is_err()
        );
    }

    #[test]
    fn accumulator_enforces_limits_before_retaining_messages() {
        let store_id = StoreId::default_blueprint(ApplicationId::from("anonymous-app"));
        let message = LogMsg::BlueprintActivationCommand(
            re_log_types::BlueprintActivationCommand::make_active(store_id.clone()),
        );
        let disabled = BlueprintAccumulator::new(store_id.clone(), 0, 1);
        assert!(disabled.is_err());

        let mut bounded = BlueprintAccumulator::new(store_id, 1, 1).unwrap();
        assert!(bounded.push(message).is_err());
    }
}
