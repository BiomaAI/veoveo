//! Governed MCP control plane for durable Rerun recordings.

pub mod admin;
mod blueprint_cache;
pub mod blueprint_playback;
pub mod contract;
pub mod live_playback;
#[cfg(feature = "redap")]
pub mod live_stream;
#[cfg(feature = "redap")]
pub mod playback;
pub mod service;
pub mod uris;

pub use service::{
    PlaybackArchiveLayerPlan, PlaybackBlueprintPlan, PlaybackLiveLayerPlan, RecordingPlaybackPlan,
    RecordingService,
};
