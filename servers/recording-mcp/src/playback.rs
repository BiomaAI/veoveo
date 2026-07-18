//! RRD playback projection for the embedded Rerun viewer.
//!
//! Physical Recording Hub segments are an internal durability boundary. The
//! viewer receives one normalized logical stream so video caches and GoP
//! metadata do not inherit those physical boundaries.

use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, ensure};
use re_chunk_store::{CompactionOptions, IsStartOfGop, OptimizationProfile};
use re_entity_db::EntityDb;
use re_log_encoding::{DecoderApp, Encoder};
use re_log_types::StoreId;

pub(crate) fn normalized_rrd(segment_paths: &[PathBuf]) -> Result<Vec<u8>> {
    ensure!(
        !segment_paths.is_empty(),
        "playback normalization requires at least one segment"
    );

    let profile = OptimizationProfile::OBJECT_STORE;
    let mut store_config = profile.to_chunk_store_config();
    store_config.enable_changelog = false;
    let mut stores = HashMap::<StoreId, EntityDb>::new();

    for path in segment_paths {
        decode_segment(path, &store_config, &mut stores)?;
    }

    let is_start_of_gop: IsStartOfGop = Arc::new(|data, codec| {
        ensure!(
            codec == re_sdk_types::components::VideoCodec::H264,
            "only H.264 playback normalization is supported"
        );
        veoveo_recording_hub::h264_access_unit_is_idr(data)
    });
    let options = CompactionOptions {
        config: store_config,
        num_extra_passes: Some(profile.num_extra_passes as usize),
        is_start_of_gop: Some(is_start_of_gop),
        split_size_ratio: profile.split_size_ratio,
        fix_keyframe: true,
    };
    for database in stores.values() {
        // Safety: this headless projection exclusively owns every EntityDb.
        #[expect(unsafe_code)]
        let engine = unsafe { database.storage_engine_raw() };
        let compacted = engine
            .read()
            .store()
            .compacted(&options)
            .context("normalizing playback chunks")?;
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
    Encoder::encode(blueprints.chain(recordings)).context("encoding normalized playback RRD")
}

fn decode_segment(
    path: &Path,
    store_config: &re_chunk_store::ChunkStoreConfig,
    stores: &mut HashMap<StoreId, EntityDb>,
) -> Result<()> {
    let file =
        File::open(path).with_context(|| format!("opening playback segment {}", path.display()))?;
    let messages = DecoderApp::decode_eager(BufReader::new(file))
        .with_context(|| format!("decoding playback segment {}", path.display()))?;
    for message in messages {
        let message =
            message.with_context(|| format!("decoding playback segment {}", path.display()))?;
        let database = stores.entry(message.store_id().clone()).or_insert_with(|| {
            EntityDb::with_store_config(message.store_id().clone(), false, store_config.clone())
        });
        database
            .add_log_msg(&message)
            .with_context(|| format!("indexing playback segment {}", path.display()))?;
    }
    Ok(())
}
