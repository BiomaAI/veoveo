//! Governed MCP control plane for durable Rerun recordings.

pub mod contract;
mod playback;
pub mod service;
pub mod uris;

pub use service::{
    RecordingReadAuthority, RecordingReadPlan, RecordingReadSegment, RecordingService,
};
