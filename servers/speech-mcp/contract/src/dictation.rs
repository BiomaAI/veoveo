//! Private, bounded microphone session. PCM is binary and never a tool argument.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_CHUNK_BYTES: usize = 192_000;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartDictation {
    pub id: Uuid,
    pub sample_rate: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DictationId {
    pub id: Uuid,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DictationStatus {
    Listening,
    Flushing,
    Completed,
    Cancelled,
    Failed,
}
impl DictationStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DictationSnapshot {
    pub id: Uuid,
    pub result_uri: String,
    pub status: DictationStatus,
    pub next_sequence: u32,
    pub max_duration_seconds: u32,
    pub transcript: Option<super::transcript::Transcript>,
}
