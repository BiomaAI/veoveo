//! Governed MCP control plane for durable Rerun recordings.

pub mod admin;
pub mod blueprint_playback;
pub mod contract;
pub mod live_playback;
#[cfg(feature = "redap")]
pub mod playback;
pub mod service;
pub mod uris;

pub use service::{
    MaterializedRecordingReadSnapshot, PlaybackArchiveSegmentPlan, PlaybackBlueprintPlan,
    PlaybackLiveSegmentPlan, RecordingPlaybackPlan, RecordingReadAuthority, RecordingReadPlan,
    RecordingReadSegment, RecordingReadSnapshot, RecordingReadSource, RecordingReadSourceKind,
    RecordingService,
};
