//! Finite, recording-scoped delivery of a producer-authored Rerun Blueprint.

use std::{io::Cursor, path::Path};

use anyhow::{Context, Result, ensure};
use re_build_info::CrateVersion;
use re_log_encoding::{Decoder, EncodingOptions, rrd::Encoder};
use re_log_types::{LogMsg, StoreId, StoreKind};
use sha2::{Digest, Sha256};

/// Decode and re-encode one validated Blueprint while projecting its store
/// identity onto the governed playback application's identity.
pub fn recording_scoped_blueprint(
    path: &Path,
    application_id: &str,
    expected_blueprint_id: &str,
    expected_byte_len: u64,
    expected_sha256: &str,
) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("reading playback Blueprint {}", path.display()))?;
    ensure!(
        bytes.len() as u64 == expected_byte_len
            && hex::encode(Sha256::digest(&bytes)) == expected_sha256,
        "playback Blueprint bytes no longer match their governed publication"
    );
    let decoder = Decoder::<LogMsg>::decode_eager(Cursor::new(bytes))
        .with_context(|| format!("decoding playback Blueprint {}", path.display()))?;
    let playback_store = StoreId::new(StoreKind::Blueprint, application_id, expected_blueprint_id);
    let mut output = Encoder::new_eager(
        CrateVersion::LOCAL,
        EncodingOptions::PROTOBUF_COMPRESSED,
        Vec::new(),
    )
    .context("opening playback Blueprint encoder")?;
    let mut count = 0_u64;
    for message in decoder {
        let mut message =
            message.with_context(|| format!("decoding playback Blueprint {}", path.display()))?;
        ensure!(
            message.store_id().kind() == StoreKind::Blueprint,
            "playback Blueprint contains a non-Blueprint store"
        );
        ensure!(
            message.store_id().recording_id().as_str() == expected_blueprint_id,
            "playback Blueprint identity changed after publication"
        );
        message.set_store_id(playback_store.clone());
        output.append(&message)?;
        count += 1;
    }
    ensure!(count > 0, "playback Blueprint contains no messages");
    output.finish()?;
    output
        .into_inner()
        .context("extracting recording-scoped playback Blueprint")
}

#[cfg(test)]
mod tests {
    use re_log_encoding::{Decoder, EncodingOptions, rrd::Encoder};
    use re_sdk::{RecordingStreamBuilder, blueprint::Blueprint};

    use super::*;

    #[test]
    fn rewrites_blueprint_and_activation_to_the_playback_application() {
        let (recording, storage) = RecordingStreamBuilder::new("anonymous-app")
            .recording_id("anonymous-recording")
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

        let rewritten = recording_scoped_blueprint(
            &path,
            "governed-playback",
            &blueprint_id,
            bytes.len() as u64,
            &hex::encode(Sha256::digest(&bytes)),
        )
        .unwrap();
        let decoded = Decoder::<LogMsg>::decode_eager(Cursor::new(rewritten))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(decoded.len(), messages.len());
        assert!(decoded.iter().all(|message| {
            message.store_id().kind() == StoreKind::Blueprint
                && message.store_id().application_id().as_str() == "governed-playback"
                && message.store_id().recording_id().as_str() == blueprint_id
        }));
    }
}
