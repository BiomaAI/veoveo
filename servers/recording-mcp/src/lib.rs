//! Governed MCP control plane for durable Rerun recordings.

pub mod admin;
pub mod blueprint_playback;
pub mod contract;
pub mod layer_cache;
pub mod live_playback;
#[cfg(feature = "redap")]
pub mod live_stream;
#[cfg(feature = "redap")]
pub mod playback;
pub mod service;
pub mod uris;

pub use service::{
    MaterializedRecordingReadSnapshot, PlaybackArchiveLayerPlan, PlaybackBlueprintPlan,
    PlaybackLiveLayerPlan, RecordingPlaybackPlan, RecordingReadAuthority, RecordingReadLayer,
    RecordingReadPlan, RecordingReadSnapshot, RecordingReadSource, RecordingReadSourceKind,
    RecordingService,
};
