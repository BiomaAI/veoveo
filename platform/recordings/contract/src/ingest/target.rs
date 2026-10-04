//! Recording-owned policy target forms.
use super::{RecordingIngestStreamId, RecordingProducerId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum RecordingTarget {
    RecordingProducer {
        producer: RecordingProducerId,
    },
    RecordingStream {
        producer: RecordingProducerId,
        stream_id: RecordingIngestStreamId,
    },
}
