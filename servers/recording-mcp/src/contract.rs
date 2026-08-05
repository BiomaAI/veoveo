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
    #[serde(default)]
    pub range: Option<QueryTimelineRange>,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub struct QueryTimelineRange {
    pub start: i64,
    pub end: i64,
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
    pub range: Option<QueryTimelineRange>,
    pub rows: Vec<serde_json::Value>,
    pub rows_by_recording: std::collections::BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct SealRecordingOutput {
    pub recording_id: String,
    pub manifest_artifact_uri: String,
    pub segment_artifact_uris: Vec<String>,
    pub blueprint_artifact_uri: Option<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PlaybackManifest {
    pub schema: String,
    pub recording_id: String,
    pub application_id: String,
    pub recording_key: String,
    pub state: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub access: PlaybackAccess,
    pub archive: Option<PlaybackArchive>,
    pub live: Option<PlaybackLiveSegment>,
    pub blueprint: Option<PlaybackBlueprint>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PlaybackAccess {
    pub session_id: String,
    pub redap_token: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PlaybackArchive {
    pub uri: String,
    pub dataset_id: String,
    pub segment_id: String,
    pub revision: String,
    pub rrd_version: String,
    pub optimization_profile: String,
    pub byte_len: u64,
    pub layer_count: usize,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PlaybackLiveSegment {
    pub segment_id: String,
    pub ordinal: i64,
    pub current_byte_len: u64,
    pub history_seconds: u64,
    pub video_preroll_seconds: u64,
    pub transport: PlaybackLiveTransport,
}

#[derive(Clone, Copy, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackLiveTransport {
    RerunJsChannelRrdFrames,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct PlaybackBlueprint {
    pub blueprint_id: String,
    pub revision: u64,
    pub sha256: String,
    pub byte_len: u64,
    pub map_provider: PlaybackMapProvider,
}

#[derive(Clone, Copy, Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PlaybackMapProvider {
    None,
    OpenStreetMap,
    Mapbox,
    Mixed,
}

#[derive(Clone, Debug, Serialize)]
pub struct RecordingManifest {
    pub schema: String,
    pub recording: RecordingView,
    pub segments: Vec<ManifestSegment>,
    pub blueprint: Option<ManifestBlueprint>,
    pub sealed_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ManifestBlueprint {
    pub blueprint_id: String,
    pub revision: i64,
    pub byte_len: i64,
    pub message_count: i64,
    pub sha256: String,
    pub artifact_uri: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ManifestSegment {
    pub segment_id: String,
    pub ordinal: i64,
    pub byte_len: i64,
    pub sha256: String,
    pub artifact_uri: String,
}
