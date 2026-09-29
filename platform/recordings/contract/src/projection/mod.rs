//! Recording-owned projection admission and result models.

mod query;
mod request;
mod result;

pub use query::{
    MAX_PROJECTION_BYTES, MAX_PROJECTION_COMPONENTS, MAX_PROJECTION_DEADLINE_MS,
    MAX_PROJECTION_ENTITIES, MAX_PROJECTION_ROWS, MAX_PROJECTION_SAMPLES,
    MAX_PROJECTION_SELECTOR_BYTES, RecordingProjectionQuery, RecordingProjectionQueryBuilder,
    RecordingProjectionSampling, RecordingProjectionSparseFill,
};
pub use request::{CreateRecordingProjectionRequest, CreateRecordingProjectionRequestBuilder};
pub use result::{
    RECORDING_PROJECTION_HANDLE_SCHEMA, RecordingProjectionHandle,
    RecordingProjectionHandleBuilder, RecordingProjectionHandleSchema,
    RecordingProjectionResultMetadata,
};

/// Maximum number of distinct revision-scoped coordinate frames in a projection.
pub const MAX_PROJECTION_FRAME_REFERENCES: usize = 64;

fn valid_frame_references(frames: &[veoveo_frames_contract::WorldFrameUri]) -> bool {
    frames.len() <= MAX_PROJECTION_FRAME_REFERENCES
        && frames
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == frames.len()
}
