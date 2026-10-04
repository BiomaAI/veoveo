//! Governed MCP control plane for durable Rerun recordings.

#[cfg(feature = "mcp")]
pub mod admin;
#[cfg(feature = "runtime")]
mod blueprint_cache;
#[cfg(feature = "runtime")]
pub mod blueprint_playback;
#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod live_playback;
#[cfg(feature = "redap")]
pub mod live_stream;
#[cfg(feature = "mcp")]
pub mod mcp_setup;
#[cfg(feature = "redap")]
pub mod playback;
#[cfg(feature = "runtime")]
pub mod service;
#[cfg(feature = "contract")]
pub mod uris;

#[cfg(feature = "runtime")]
pub use service::{
    PlaybackArchiveLayerPlan, PlaybackBlueprintPlan, PlaybackLiveLayerPlan, RecordingPlaybackPlan,
    RecordingService,
};

#[cfg(feature = "schema")]
pub mod schema;

#[cfg(feature = "gateway")]
pub mod gateway;
