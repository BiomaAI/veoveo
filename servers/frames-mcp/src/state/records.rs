//! Private driver records; Frames owns their domain admission.
use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_platform_store::OpenObject;

#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub(super) struct FrameWorldRecord {
    pub(super) id: RecordId,
    pub(super) tenant: RecordId,
    pub(super) owner: RecordId,
    pub(super) world_key: String,
    pub(super) display_name: String,
    pub(super) description: Option<String>,
    pub(super) head_revision: Option<RecordId>,
    pub(super) head_revision_key: Option<String>,
    pub(super) revision: i64,
    pub(super) classification: String,
    pub(super) labels: Vec<String>,
    pub(super) created_at: DateTime<Utc>,
    pub(super) updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub(super) struct FrameWorldRevisionRecord {
    pub(super) id: RecordId,
    pub(super) tenant: RecordId,
    pub(super) owner: RecordId,
    pub(super) world: RecordId,
    pub(super) world_key: String,
    pub(super) revision_key: String,
    pub(super) revision: i64,
    pub(super) spec_sha256: String,
    pub(super) root_frame_key: String,
    pub(super) definition: OpenObject,
    pub(super) frame_ids: Vec<String>,
    pub(super) created_at: DateTime<Utc>,
}
