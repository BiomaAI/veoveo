//! Blueprint identity validation used by the shared Artifact-backed RRD cache.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::path::Path;
use veoveo_recording_reader::cache::RrdIdentityValidator;

pub(crate) struct BlueprintIdentity {
    pub application_id: String,
    pub blueprint_id: String,
    pub message_count: u64,
}

impl RrdIdentityValidator for BlueprintIdentity {
    fn validate(&self, path: &Path, byte_len: u64, sha256: &str) -> Result<()> {
        ensure!(
            self.message_count > 0,
            "recording Blueprint must not be empty"
        );
        let bytes = std::fs::read(path)?;
        ensure!(
            bytes.len() as u64 == byte_len && hex::encode(Sha256::digest(&bytes)) == sha256,
            "cached Blueprint digest or length mismatch"
        );
        let blueprint = veoveo_recording_hub::validate_blueprint_rrd(
            &bytes,
            self.message_count,
            &self.application_id,
        )?;
        ensure!(
            blueprint.store_id.recording_id().as_str() == self.blueprint_id,
            "cached Blueprint identity mismatch"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use re_build_info::CrateVersion;
    use re_log_encoding::{EncodingOptions, rrd::Encoder};
    use re_log_types::StoreKind;
    use re_sdk::{RecordingStreamBuilder, blueprint::Blueprint};
    #[test]
    fn blueprint_cache_validation_binds_application_identity_and_message_count() {
        let (recording, storage) = RecordingStreamBuilder::new("cache-blueprint-app")
            .recording_id("cache-recording")
            .memory()
            .unwrap();
        Blueprint::auto()
            .send(&recording, Default::default())
            .unwrap();
        let messages = storage
            .take()
            .into_iter()
            .filter(|message| message.store_id().kind() == StoreKind::Blueprint)
            .collect::<Vec<_>>();
        let blueprint_id = messages[0].store_id().recording_id().as_str().to_owned();
        let mut encoder = Encoder::new_eager(
            CrateVersion::LOCAL,
            EncodingOptions::PROTOBUF_COMPRESSED,
            Vec::new(),
        )
        .unwrap();
        for message in &messages {
            encoder.append(message).unwrap();
        }
        encoder.finish().unwrap();
        let bytes = encoder.into_inner().unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("blueprint.rrd");
        std::fs::write(&path, &bytes).unwrap();
        let digest = hex::encode(Sha256::digest(&bytes));
        let validation = BlueprintIdentity {
            application_id: "cache-blueprint-app".to_owned(),
            blueprint_id: blueprint_id.clone(),
            message_count: messages.len() as u64,
        };

        validation
            .validate(&path, bytes.len() as u64, &digest)
            .unwrap();
        assert!(
            BlueprintIdentity {
                application_id: "other-app".to_owned(),
                blueprint_id,
                message_count: messages.len() as u64,
            }
            .validate(&path, bytes.len() as u64, &digest)
            .is_err()
        );
        assert!(
            BlueprintIdentity {
                application_id: "cache-blueprint-app".to_owned(),
                blueprint_id: messages[0].store_id().recording_id().as_str().to_owned(),
                message_count: messages.len() as u64 + 1,
            }
            .validate(&path, bytes.len() as u64, &digest)
            .is_err()
        );
    }
}
