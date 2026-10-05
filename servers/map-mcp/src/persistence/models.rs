use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_platform_store::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum MapReleaseState {
    #[vocabulary(rename = "staged")]
    Staged,
    #[vocabulary(rename = "active")]
    Active,
    #[vocabulary(rename = "retired")]
    Retired,
    #[vocabulary(rename = "quarantined")]
    Quarantined,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum MapAcquisitionState {
    #[vocabulary(rename = "queued")]
    Queued,
    #[vocabulary(rename = "running")]
    Running,
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancel_requested")]
    CancelRequested,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum MapRouteState {
    #[vocabulary(rename = "planning_advisory")]
    PlanningAdvisory,
    #[vocabulary(rename = "validated")]
    Validated,
    #[vocabulary(rename = "stale")]
    Stale,
    #[vocabulary(rename = "invalidated")]
    Invalidated,
    #[vocabulary(rename = "unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum MapDependencyKind {
    #[vocabulary(rename = "release")]
    Release,
    #[vocabulary(rename = "restriction")]
    Restriction,
    #[vocabulary(rename = "facility")]
    Facility,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapSourceRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub source_key: String,
    pub dataset_key: String,
    pub name: String,
    pub adapter_kind: String,
    pub authority_class: String,
    pub map_families: Vec<String>,
    pub enabled: bool,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapDatasetReleaseRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub release_key: String,
    pub dataset_key: String,
    pub source_key: String,
    pub state: MapReleaseState,
    pub version_label: String,
    pub source_digest_sha256: String,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapActiveReleaseRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub dataset_key: String,
    pub release_key: String,
    pub previous_release_key: Option<String>,
    pub activated_by: RecordId,
    pub activated_at: DateTime<Utc>,
    pub record_version: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapMobilityProfileRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub profile_key: String,
    pub family: String,
    pub name: String,
    pub profile_version: i64,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapRestrictionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub restriction_key: String,
    pub kind: String,
    pub effect_kind: String,
    pub affected_mobility_families: Vec<String>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub cancelled_by: Option<String>,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapOperationalSnapshotRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub snapshot_key: String,
    pub departure_time: DateTime<Utc>,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapRouteRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub route_key: String,
    pub status: MapRouteState,
    pub mobility_profile_key: String,
    pub mobility_profile_version: i64,
    pub operational_snapshot_key: String,
    pub departure_time: DateTime<Utc>,
    pub arrival_time: Option<DateTime<Utc>>,
    pub cache_digest_sha256: String,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapRouteDependencyRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub route_key: String,
    pub dependency_kind: MapDependencyKind,
    pub dependency_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapRouteMatrixRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub matrix_key: String,
    pub mobility_profile_key: String,
    pub mobility_profile_version: i64,
    pub operational_snapshot_key: String,
    pub artifact_uri: Option<String>,
    pub canonical_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapAcquisitionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub acquisition_key: String,
    pub source_key: String,
    pub idempotency_key: String,
    pub status: MapAcquisitionState,
    pub phase: String,
    pub staged_release_key: Option<String>,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapFeatureLayerRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub work_context: RecordId,
    pub authority: InvocationAuthorityRecord,
    pub created_by_key: String,
    pub owner_kind: ArtifactGrantSubjectKind,
    pub owner_key: String,
    pub layer_key: String,
    pub title: String,
    pub description: Option<String>,
    pub content_class: String,
    pub schema_version: i64,
    pub schema_revision_key: String,
    pub style_version: Option<i64>,
    pub style_revision_key: Option<String>,
    pub revision: i64,
    pub classification: Option<String>,
    pub data_labels: Vec<String>,
    pub archived_at: Option<DateTime<Utc>>,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapFeatureSchemaRevisionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub layer_key: String,
    pub schema_revision_key: String,
    pub schema_version: i64,
    pub digest_sha256: String,
    pub schema_json: String,
    pub created_by: RecordId,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapStyleRevisionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub layer_key: String,
    pub style_revision_key: String,
    pub style_version: i64,
    pub style_json: String,
    pub created_by: RecordId,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapFeatureHeadRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub layer_key: String,
    pub feature_key: String,
    pub feature_revision: i64,
    pub layer_revision: i64,
    pub schema_version: i64,
    pub changeset_key: String,
    pub deleted: bool,
    pub geometry_type: String,
    pub geometry_json: String,
    pub bbox_west: f64,
    pub bbox_south: f64,
    pub bbox_east: f64,
    pub bbox_north: f64,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub semantic_type: String,
    pub title: Option<String>,
    pub canonical_json: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapFeatureRevisionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub layer_key: String,
    pub feature_key: String,
    pub feature_revision: i64,
    pub layer_revision: i64,
    pub schema_version: i64,
    pub changeset_key: String,
    pub deleted: bool,
    pub geometry_type: String,
    pub geometry_json: String,
    pub bbox_west: f64,
    pub bbox_south: f64,
    pub bbox_east: f64,
    pub bbox_north: f64,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub semantic_type: String,
    pub title: Option<String>,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapFeatureChangeSetRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub work_context: RecordId,
    pub actor_key: String,
    pub work_context_key: String,
    pub authority: InvocationAuthorityRecord,
    pub layer_key: String,
    pub changeset_key: String,
    pub base_layer_revision: i64,
    pub resulting_layer_revision: i64,
    pub feature_keys: Vec<String>,
    pub idempotency_key: String,
    pub request_digest_sha256: String,
    pub commit_sequence: i64,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapLayerPublicationRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub work_context: RecordId,
    pub published_by_key: String,
    pub work_context_key: String,
    pub authority: InvocationAuthorityRecord,
    pub publication_key: String,
    pub layer_key: String,
    pub layer_revision: i64,
    pub schema_version: i64,
    pub style_revision_key: Option<String>,
    pub artifact_uris: Vec<String>,
    pub canonical_json: String,
    pub published_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapLayerProductRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub work_context: RecordId,
    pub authority: InvocationAuthorityRecord,
    pub product_key: String,
    pub publication_key: String,
    pub layer_key: String,
    pub layer_revision: i64,
    pub format: String,
    pub artifact_uri: String,
    pub mime_type: String,
    pub digest_sha256: String,
    pub size_bytes: i64,
    pub feature_count: i64,
    pub canonical_json: String,
    pub created_by_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapCompositionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub work_context: RecordId,
    pub authority: InvocationAuthorityRecord,
    pub created_by_key: String,
    pub owner_kind: ArtifactGrantSubjectKind,
    pub owner_key: String,
    pub composition_key: String,
    pub title: String,
    pub current_revision: i64,
    pub canonical_json: String,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapCompositionRevisionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub authority: InvocationAuthorityRecord,
    pub composition_key: String,
    pub composition_revision_key: String,
    pub revision: i64,
    pub publication_keys: Vec<String>,
    pub canonical_json: String,
    pub created_by: RecordId,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
#[path = "vocabulary_baseline.rs"]
mod vocabulary_baseline;
