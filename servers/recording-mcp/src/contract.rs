use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueryRecordingRequest {
    pub recording_id: String,
    #[serde(default = "default_entities")]
    pub entities: String,
    #[serde(default = "default_timeline")]
    pub timeline: String,
    #[serde(default = "default_max_rows")]
    pub max_rows: u64,
}

fn default_entities() -> String {
    "/**".to_owned()
}

fn default_timeline() -> String {
    "tick".to_owned()
}

fn default_max_rows() -> u64 {
    10_000
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SealRecordingRequest {
    pub recording_id: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct RecordingView {
    pub recording_id: String,
    pub dataset: String,
    pub application_id: String,
    pub recording_key: String,
    pub state: String,
    pub classification: String,
    pub labels: Vec<String>,
    pub started_at: String,
    pub last_data_at: String,
    pub ended_at: Option<String>,
    pub sealed_at: Option<String>,
    pub manifest_artifact_uri: Option<String>,
    pub segment_count: usize,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct SegmentView {
    pub segment_id: String,
    pub ordinal: i64,
    pub state: String,
    pub byte_len: i64,
    pub message_count: i64,
    pub sha256: Option<String>,
    pub artifact_uri: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct QueryRecordingOutput {
    pub recording_id: String,
    pub timeline: String,
    pub rows: Vec<serde_json::Value>,
    pub rows_by_recording: std::collections::BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct SealRecordingOutput {
    pub recording_id: String,
    pub manifest_artifact_uri: String,
    pub segment_artifact_uris: Vec<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PlaybackManifest {
    pub recording_id: String,
    pub application_id: String,
    pub recording_key: String,
    pub state: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub segments: Vec<PlaybackSegment>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PlaybackSegment {
    pub segment_id: String,
    pub ordinal: i64,
    pub byte_len: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecordingManifest {
    pub schema: String,
    pub recording: RecordingView,
    pub segments: Vec<ManifestSegment>,
    pub sealed_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ManifestSegment {
    pub segment_id: String,
    pub ordinal: i64,
    pub byte_len: i64,
    pub sha256: String,
    pub artifact_uri: String,
}
