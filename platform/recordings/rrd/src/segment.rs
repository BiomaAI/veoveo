//! Verify the bytes and producer identity of one complete RRD segment.

use anyhow::{Context, Result, ensure};
use re_dataframe::{ChunkStoreConfig, QueryEngine};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SegmentInspection {
    pub application_id: String,
    pub recording_key: String,
    pub byte_len: u64,
    pub sha256: String,
}

pub fn inspect_segment(path: &Path) -> Result<SegmentInspection> {
    let file = File::open(path).with_context(|| format!("opening segment {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("syncing segment {}", path.display()))?;
    let byte_len = file
        .metadata()
        .with_context(|| format!("reading segment metadata {}", path.display()))?
        .len();
    ensure!(byte_len > 0, "segment {} is empty", path.display());

    let mut hash = Sha256::new();
    let mut reader = BufReader::new(file);
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .with_context(|| format!("hashing segment {}", path.display()))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let sha256 = hex::encode(hash.finalize());

    let engines = QueryEngine::from_rrd_filepath(&ChunkStoreConfig::DEFAULT, path)
        .with_context(|| format!("validating RRD segment {}", path.display()))?;
    let mut identities = engines
        .into_iter()
        .filter(|(store_id, _)| store_id.is_recording())
        .map(|(store_id, _)| {
            (
                store_id.application_id().as_str().to_owned(),
                store_id.recording_id().as_str().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    ensure!(
        identities.len() == 1,
        "segment {} must contain exactly one recording identity",
        path.display()
    );
    let (application_id, recording_key) = identities.remove(0);
    Ok(SegmentInspection {
        application_id,
        recording_key,
        byte_len,
        sha256,
    })
}
