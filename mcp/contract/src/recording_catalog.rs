//! Shared HTTP contracts for governed recording catalogs.

use schemars::JsonSchema;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RECORDING_CATALOG_GRANT_SCHEMA: &str = "veoveo.io/recording-catalog-grant/v1";
pub const RECORDING_PROJECTION_HANDLE_SCHEMA: &str = "veoveo.io/recording-projection-handle/v1";

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateRecordingCatalogGrantRequest {
    #[schemars(with = "String")]
    pub dataset_id: Uuid,
    #[schemars(with = "Vec<String>")]
    pub recording_ids: Vec<Uuid>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingCatalogGrant {
    pub schema: String,
    #[schemars(with = "String")]
    pub grant_id: Uuid,
    #[schemars(with = "String")]
    pub dataset_id: Uuid,
    #[schemars(with = "Vec<String>")]
    pub recording_segment_ids: Vec<Uuid>,
    pub catalog_revision: String,
    pub entry_uri: String,
    pub redap_token: String,
    pub expires_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordingProjectionSparseFill {
    None,
    LatestAtGlobal,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RecordingProjectionSampling {
    Range { start: i64, end: i64 },
    LatestAt { at: i64 },
    SampleGrid { values: Vec<i64> },
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateRecordingProjectionRequest {
    #[schemars(with = "String")]
    pub dataset_id: Uuid,
    #[schemars(with = "String")]
    pub recording_id: Uuid,
    pub entity_paths: Vec<String>,
    pub component_ids: Vec<String>,
    pub timeline: String,
    pub sampling: RecordingProjectionSampling,
    pub sparse_fill: RecordingProjectionSparseFill,
    pub maximum_entities: usize,
    pub maximum_columns: usize,
    pub maximum_samples: usize,
    pub maximum_rows: u64,
    pub maximum_bytes: u64,
    pub deadline_ms: u64,
    pub idempotency_key: String,
    pub units: BTreeMap<String, String>,
    pub coordinate_frame_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingProjectionResultMetadata {
    pub catalog_revision: String,
    pub query_digest: String,
    pub timeline: String,
    pub sample_grid: Vec<i64>,
    pub units: BTreeMap<String, String>,
    pub coordinate_frame_refs: Vec<String>,
    pub omitted_sample_count: u64,
    pub row_count: u64,
    pub arrow_schema_sha256: String,
    pub byte_len: u64,
    pub payload_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingProjectionHandle {
    pub schema: String,
    #[schemars(with = "String")]
    pub projection_id: Uuid,
    #[schemars(with = "String")]
    pub dataset_id: Uuid,
    #[schemars(with = "String")]
    pub recording_id: Uuid,
    pub result: RecordingProjectionResultMetadata,
    pub expires_at: String,
}
