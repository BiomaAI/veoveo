//! One-time materialization of immutable, footer-indexed archive shards.
//!
//! Durable ingest parts and direct live files are write-path formats. Before a
//! segment becomes readable, this module rewrites it once with Rerun's
//! object-store profile. The result is the only format exposed by archive
//! playback: large query-oriented chunks, thick/thin column separation,
//! GoP-aligned video chunks, repaired keyframe metadata, and an RRD footer.

use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{BufReader, Write as _},
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, ensure};
use re_chunk_store::{CompactionOptions, IsStartOfGop, OptimizationProfile};
use re_entity_db::EntityDb;
use re_log_encoding::{DecoderApp, Encoder};
use re_log_types::StoreId;

/// Statistics from one archive materialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveMaterialization {
    pub input_messages: u64,
    pub output_bytes: u64,
}

/// Compact one or more complete RRD inputs into one independently valid shard.
///
/// Each Rerun store is compacted independently and encoded in stable identifier
/// order. Inputs for one shard must still describe one logical recording.
///
/// The destination is published atomically only after the optimized RRD is
/// fully written and fsynced.
pub fn materialize_archive_shard(
    input_paths: &[PathBuf],
    destination: &Path,
) -> Result<ArchiveMaterialization> {
    ensure!(
        !input_paths.is_empty(),
        "archive materialization requires at least one input"
    );

    let profile = OptimizationProfile::OBJECT_STORE;
    let mut store_config = profile.to_chunk_store_config();
    store_config.enable_changelog = false;
    let mut stores = BTreeMap::<StoreId, EntityDb>::new();
    let mut input_messages = 0_u64;

    for path in input_paths {
        let file =
            File::open(path).with_context(|| format!("opening RRD input {}", path.display()))?;
        let messages = DecoderApp::decode_eager(BufReader::new(file))
            .with_context(|| format!("decoding RRD input {}", path.display()))?;
        for message in messages {
            let message =
                message.with_context(|| format!("decoding RRD input {}", path.display()))?;
            let database = stores.entry(message.store_id().clone()).or_insert_with(|| {
                EntityDb::with_store_config(message.store_id().clone(), false, store_config.clone())
            });
            database
                .add_log_msg(&message)
                .with_context(|| format!("indexing RRD input {}", path.display()))?;
            input_messages = input_messages
                .checked_add(1)
                .context("archive input message count overflow")?;
        }
    }
    ensure!(!stores.is_empty(), "archive inputs contain no Rerun stores");

    let is_start_of_gop: IsStartOfGop = Arc::new(|data, codec| {
        ensure!(
            codec == re_sdk_types::components::VideoCodec::H264,
            "archive materialization supports H.264 VideoStream data"
        );
        crate::h264_access_unit_is_idr(data)
    });
    let options = CompactionOptions {
        config: store_config,
        num_extra_passes: Some(profile.num_extra_passes as usize),
        is_start_of_gop: Some(is_start_of_gop),
        split_size_ratio: profile.split_size_ratio,
        fix_keyframe: true,
    };
    for database in stores.values() {
        // Safety: this materializer exclusively owns each headless EntityDb.
        #[expect(unsafe_code)]
        let engine = unsafe { database.storage_engine_raw() };
        let compacted = engine
            .read()
            .store()
            .compacted(&options)
            .context("compacting archive shard")?;
        *engine.write().store() = compacted;
    }

    let blueprints = stores
        .values()
        .filter(|database| database.store_id().is_blueprint())
        .flat_map(|database| database.to_messages(None));
    let recordings = stores
        .values()
        .filter(|database| database.store_id().is_recording())
        .flat_map(|database| database.to_messages(None));
    let bytes =
        Encoder::encode(blueprints.chain(recordings)).context("encoding optimized archive RRD")?;
    publish_atomic(destination, &bytes)?;
    Ok(ArchiveMaterialization {
        input_messages,
        output_bytes: u64::try_from(bytes.len()).context("archive output exceeds u64")?,
    })
}

fn publish_atomic(destination: &Path, bytes: &[u8]) -> Result<()> {
    let parent = destination
        .parent()
        .with_context(|| format!("archive path {} has no parent", destination.display()))?;
    std::fs::create_dir_all(parent)?;
    let temporary =
        destination.with_extension(format!("rrd.{}.materializing", uuid::Uuid::now_v7()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .with_context(|| format!("creating archive temporary {}", temporary.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&temporary, destination).with_context(|| {
        format!(
            "publishing archive shard {} over {}",
            temporary.display(),
            destination.display()
        )
    })?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::BufReader;

    use re_log_encoding::{Decoder, EncodingOptions, rrd::Encoder};
    use re_log_types::LogMsg;
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;

    use super::*;

    #[test]
    fn materializes_many_parts_as_one_footer_indexed_rrd() {
        let (recording, storage) = RecordingStreamBuilder::new("archive-test")
            .recording_id("recording-a")
            .memory()
            .unwrap();
        for value in 0..8 {
            recording
                .log("sensor/value", &Scalars::single(value as f64))
                .unwrap();
        }
        let messages = storage.take();
        let directory = tempfile::tempdir().unwrap();
        let mut parts = Vec::new();
        for (ordinal, message) in messages.iter().enumerate() {
            let path = directory.path().join(format!("{ordinal}.rrd"));
            let mut encoder = Encoder::new_eager(
                re_build_info::CrateVersion::LOCAL,
                EncodingOptions::PROTOBUF_COMPRESSED,
                Vec::new(),
            )
            .unwrap();
            encoder.append(message).unwrap();
            encoder.finish().unwrap();
            std::fs::write(&path, encoder.into_inner().unwrap()).unwrap();
            parts.push(path);
        }

        let output = directory.path().join("archive.rrd");
        let result = materialize_archive_shard(&parts, &output).unwrap();
        assert_eq!(result.input_messages, messages.len() as u64);
        assert_eq!(
            result.output_bytes,
            std::fs::metadata(&output).unwrap().len()
        );
        assert!(
            re_log_encoding::read_rrd_footer(&mut File::open(&output).unwrap())
                .unwrap()
                .is_some(),
            "archive has a readable footer manifest"
        );
        assert!(
            Decoder::<LogMsg>::decode_eager(BufReader::new(File::open(&output).unwrap()))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
                .len()
                < messages.len()
        );
    }
}
