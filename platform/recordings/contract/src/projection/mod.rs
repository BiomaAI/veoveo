//! Recording-owned projection admission and result models.

mod query;
mod request;

pub use query::{
    MAX_PROJECTION_BYTES, MAX_PROJECTION_COMPONENTS, MAX_PROJECTION_DEADLINE_MS,
    MAX_PROJECTION_ENTITIES, MAX_PROJECTION_ROWS, MAX_PROJECTION_SAMPLES,
    MAX_PROJECTION_SELECTOR_BYTES, RecordingProjectionQuery, RecordingProjectionQueryBuilder,
    RecordingProjectionSampling, RecordingProjectionSparseFill,
};
pub use request::{CreateRecordingProjectionRequest, CreateRecordingProjectionRequestBuilder};
