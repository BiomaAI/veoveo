use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_platform_store::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum RecordingState {
    #[vocabulary(rename = "live")]
    Live,
    #[vocabulary(rename = "ready")]
    Ready,
    #[vocabulary(rename = "sealing")]
    Sealing,
    #[vocabulary(rename = "sealed")]
    Sealed,
    #[vocabulary(rename = "interrupted")]
    Interrupted,
    #[vocabulary(rename = "failed")]
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum RecordingRetentionMode {
    #[vocabulary(rename = "installation_default")]
    InstallationDefault,
    #[vocabulary(rename = "retain_until")]
    RetainUntil,
    #[vocabulary(rename = "retain_forever")]
    RetainForever,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum RecordingLayerKind {
    #[vocabulary(rename = "capture")]
    Capture,
    #[vocabulary(rename = "properties")]
    Properties,
    #[vocabulary(rename = "derived")]
    Derived,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum RecordingLayerState {
    #[vocabulary(rename = "writing")]
    Writing,
    #[vocabulary(rename = "staged")]
    Staged,
    #[vocabulary(rename = "committed")]
    Committed,
    #[vocabulary(rename = "failed")]
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum RecordingReadGrantClass {
    #[vocabulary(rename = "viewer_segment")]
    ViewerSegment,
    #[vocabulary(rename = "catalog_dataset")]
    CatalogDataset,
    #[vocabulary(rename = "app_projection")]
    AppProjection,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum RecordingProjectionState {
    #[vocabulary(rename = "reserved")]
    Reserved,
    #[vocabulary(rename = "materializing")]
    Materializing,
    #[vocabulary(rename = "ready")]
    Ready,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum RecordingIngestStreamState {
    #[vocabulary(rename = "open")]
    Open,
    #[vocabulary(rename = "finished")]
    Finished,
    #[vocabulary(rename = "failed")]
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum RecordingIngestBatchState {
    #[vocabulary(rename = "durable")]
    Durable,
    #[vocabulary(rename = "materialized")]
    Materialized,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub work_context: RecordId,
    pub initiator: Option<RecordId>,
    pub invocation_mode: InvocationMode,
    pub delegation_id: Option<String>,
    pub policy_revision: String,
    pub authority: InvocationAuthorityRecord,
    pub dataset: RecordId,
    pub application_id: String,
    pub recording_key: String,
    pub state: RecordingState,
    pub classification: String,
    pub labels: Vec<String>,
    pub metadata: OpenObject,
    pub manifest_artifact: Option<RecordId>,
    pub seal_task: Option<RecordId>,
    pub failure_reason: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_data_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub sealed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingDatasetRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub dataset_key: String,
    pub display_label: String,
    pub default_blueprint_artifact: Option<RecordId>,
    pub retention_mode: RecordingRetentionMode,
    pub retention_expires_at: Option<DateTime<Utc>>,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingLayerRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub recording: RecordId,
    pub layer_name: String,
    pub kind: RecordingLayerKind,
    pub ordinal: Option<i64>,
    pub staging_path: Option<String>,
    pub artifact: Option<RecordId>,
    pub state: RecordingLayerState,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub byte_len: i64,
    pub message_count: i64,
    pub sha256: Option<String>,
    pub rrd_version: Option<String>,
    pub schema_digest: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub properties_preparation: Option<crate::RecordingPropertiesPreparation>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingReadGrantRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub dataset: RecordId,
    pub grant_class: RecordingReadGrantClass,
    pub recordings: Vec<RecordId>,
    pub admitted_set_digest: String,
    pub actor: RecordId,
    pub work_context: RecordId,
    pub policy_revision: String,
    pub catalog_revision: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingProjectionReceiptRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub grant: RecordId,
    pub dataset: RecordId,
    pub recordings: Vec<RecordId>,
    pub actor: RecordId,
    pub work_context: RecordId,
    pub policy_revision: String,
    pub catalog_revision: String,
    pub caller_idempotency_key: String,
    pub manifest_digest: String,
    pub query_digest: String,
    pub state: RecordingProjectionState,
    pub result_byte_len: Option<i64>,
    pub result_sha256: Option<String>,
    pub failure_reason: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingIngestStreamRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub recording: RecordId,
    pub producer_id: String,
    pub oauth_client_id: String,
    pub source_stream_id: String,
    pub application_id: String,
    pub recording_key: String,
    pub dataset: String,
    pub state: RecordingIngestStreamState,
    pub next_sequence: i64,
    pub materialized_through_sequence: Option<i64>,
    pub byte_len: i64,
    pub message_count: i64,
    pub failure_reason: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingIngestBatchRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub stream: RecordId,
    pub sequence: i64,
    pub payload_format: String,
    pub sha256: String,
    pub relative_path: String,
    pub byte_len: i64,
    pub message_count: i64,
    pub state: RecordingIngestBatchState,
    pub created_at: DateTime<Utc>,
    pub materialized_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingBlueprintRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub recording: RecordId,
    pub stream: Option<RecordId>,
    pub artifact: Option<RecordId>,
    pub owner: RecordId,
    pub work_context: RecordId,
    pub producer_id: String,
    pub application_id: String,
    pub blueprint_id: String,
    pub revision: i64,
    pub relative_path: String,
    pub sha256: String,
    pub byte_len: i64,
    pub message_count: i64,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
#[path = "vocabulary_baseline.rs"]
mod vocabulary_baseline;
