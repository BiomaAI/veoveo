//! Recording domain contracts shared by producers, playback and analysis consumers.

pub mod uris;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod cursor;
mod ids;
mod resources;
mod scopes;
pub use cursor::{RECORDING_PAGE_SIZE, RecordingCatalogCursor};
pub use ids::{
    RecordingContractError, RecordingDatasetId, RecordingId, RecordingLayerId,
    RecordingProjectionId, RecordingReadGrantId,
};
pub use resources::{RecordingDocument, RecordingLayersUri, RecordingResource, RecordingUri};
pub use scopes::RecordingScope;

pub const PLAYBACK_MANIFEST_SCHEMA: &str = "veoveo.ai/recording-playback/v9";

mod catalog;
pub use catalog::{
    CreateRecordingCatalogGrantRequest, CreateRecordingProjectionRequest,
    RECORDING_CATALOG_GRANT_SCHEMA, RECORDING_PROJECTION_HANDLE_SCHEMA, RecordingCatalogGrant,
    RecordingProjectionHandle, RecordingProjectionResultMetadata, RecordingProjectionSampling,
    RecordingProjectionSparseFill,
};

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SealRecordingRequest {
    pub recording_id: RecordingId,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct RecordingView {
    pub recording_id: RecordingId,
    pub dataset_id: RecordingDatasetId,
    pub dataset_key: String,
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
    pub layer_count: usize,
    pub committed_layer_count: usize,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct RecordingCatalogPage {
    pub items: Vec<RecordingView>,
    pub limit: usize,
    pub next_cursor: Option<RecordingCatalogCursor>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct LayerView {
    pub layer_id: RecordingLayerId,
    pub layer_name: String,
    pub kind: String,
    pub ordinal: Option<i64>,
    pub state: String,
    pub byte_len: i64,
    pub message_count: i64,
    pub sha256: Option<String>,
    pub artifact_uri: Option<String>,
    pub rrd_version: Option<String>,
    pub schema_digest: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct SealRecordingOutput {
    pub recording_id: RecordingId,
    pub manifest_artifact_uri: String,
    pub layer_artifact_uris: Vec<String>,
    pub blueprint_artifact_uri: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaybackManifest {
    pub schema: String,
    pub dataset_id: RecordingDatasetId,
    pub recording_segment_id: RecordingId,
    pub application_id: String,
    pub recording_key: String,
    pub state: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub catalog_revision: String,
    pub access: PlaybackAccess,
    pub archive: Option<PlaybackArchive>,
    pub live: Option<PlaybackLiveReceiver>,
    pub blueprint: Option<PlaybackBlueprint>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaybackAccess {
    pub grant_id: RecordingReadGrantId,
    pub redap_token: String,
    pub expires_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaybackArchive {
    pub uri: String,
    pub dataset_id: RecordingDatasetId,
    pub recording_segment_id: RecordingId,
    pub catalog_revision: String,
    pub rrd_version: String,
    pub optimization_profile: String,
    pub byte_len: u64,
    pub layer_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaybackLiveReceiver {
    pub layer_id: RecordingLayerId,
    pub layer_name: String,
    pub ordinal: i64,
    pub current_byte_len: u64,
    pub history_seconds: u64,
    pub video_preroll_seconds: u64,
    pub transport: PlaybackLiveTransport,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackLiveTransport {
    RerunRrdChannelV2,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaybackBlueprint {
    pub blueprint_id: String,
    pub revision: u64,
    pub sha256: String,
    pub byte_len: u64,
    pub map_provider: PlaybackMapProvider,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
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
    pub dataset_id: RecordingDatasetId,
    pub recording_segment_id: RecordingId,
    pub catalog_revision: String,
    pub layers: Vec<ManifestLayer>,
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
pub struct ManifestLayer {
    pub layer_id: RecordingLayerId,
    pub layer_name: String,
    pub kind: String,
    pub ordinal: Option<i64>,
    pub byte_len: i64,
    pub sha256: String,
    pub artifact_uri: String,
    pub rrd_version: Option<String>,
    pub schema_digest: Option<String>,
}
