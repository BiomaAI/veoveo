//! Recording domain contracts shared by producers, playback and analysis consumers.

pub mod uris;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod cursor;
mod hex_digest;
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

mod playback;
pub use playback::{
    PLAYBACK_MANIFEST_SCHEMA, PlaybackAccess, PlaybackArchive, PlaybackBlueprint,
    PlaybackLiveReceiver, PlaybackLiveTransport, PlaybackManifest, PlaybackManifestBuilder,
    PlaybackManifestSchema, PlaybackMapProvider, RecordingState,
};

mod catalog;
pub use catalog::{
    CreateRecordingCatalogGrantRequest, RECORDING_CATALOG_GRANT_SCHEMA, RecordingCatalogGrant,
};
mod projection;
pub use projection::{
    CreateRecordingProjectionRequest, CreateRecordingProjectionRequestBuilder,
    MAX_PROJECTION_BYTES, MAX_PROJECTION_COMPONENTS, MAX_PROJECTION_DEADLINE_MS,
    MAX_PROJECTION_ENTITIES, MAX_PROJECTION_ROWS, MAX_PROJECTION_SAMPLES,
    MAX_PROJECTION_SELECTOR_BYTES, RECORDING_PROJECTION_HANDLE_SCHEMA, RecordingProjectionHandle,
    RecordingProjectionHandleBuilder, RecordingProjectionHandleSchema, RecordingProjectionQuery,
    RecordingProjectionQueryBuilder, RecordingProjectionResultMetadata,
    RecordingProjectionSampling, RecordingProjectionSparseFill,
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
    pub state: RecordingState,
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
