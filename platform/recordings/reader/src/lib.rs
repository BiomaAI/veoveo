//! Governed recording analysis, independent of Hub and MCP service lifecycles.
use anyhow::{Context, Result, ensure};
use std::path::PathBuf;
use veoveo_platform_store::PlatformStore;

pub mod access;
pub mod cache;
mod read;
pub mod uris;
pub use read::{
    MaterializedRecordingReadSnapshot, RecordingReadAuthority, RecordingReadLayer,
    RecordingReadPlan, RecordingReadSnapshot, RecordingReadSource, RecordingReadSourceKind,
};

pub const MAX_LAYERS: u32 = 10_000;

#[derive(Clone)]
pub struct RecordingReader {
    store: PlatformStore,
    spool_root: PathBuf,
    layer_cache: Option<cache::LayerCache>,
}

impl RecordingReader {
    pub fn new(
        store: PlatformStore,
        spool_root: PathBuf,
        layer_cache: Option<cache::LayerCache>,
    ) -> Result<Self> {
        ensure!(
            spool_root.is_absolute(),
            "recording spool root must be absolute"
        );
        let spool_root = spool_root
            .canonicalize()
            .with_context(|| format!("canonicalizing spool root {}", spool_root.display()))?;
        Ok(Self {
            store,
            spool_root,
            layer_cache,
        })
    }
}
