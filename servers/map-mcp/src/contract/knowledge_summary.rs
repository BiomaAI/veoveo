//! Bounded discovery documents link to complete Map records and hash their bytes.
use super::{
    DatasetReleaseState, FacilityKind, FeatureContentClass, Wgs84BoundingBox, Wgs84Position,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceUri, Sha256Digest};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MapKnowledgeSummary {
    pub source: ResourceUri,
    pub source_sha256: Sha256Digest,
    pub title: String,
    pub details: MapKnowledgeDetails,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields,
    rename_all_fields = "camelCase"
)]
pub enum MapKnowledgeDetails {
    Layer {
        description: Option<String>,
        content_class: FeatureContentClass,
        revision: u64,
        archived: bool,
        schema_version: u64,
    },
    Feature {
        semantic_type: String,
        bounds: Wgs84BoundingBox,
        feature_revision: u64,
        layer_revision: u64,
        deleted: bool,
        properties: Vec<MapPropertyExcerpt>,
        omitted_properties: usize,
    },
    Publication {
        layer_revision: u64,
        schema_version: u64,
        artifact_count: usize,
    },
    Location {
        position: Wgs84Position,
        alternate_names: Vec<String>,
        omitted_names: usize,
    },
    Facility {
        position: Wgs84Position,
        facility_kind: FacilityKind,
    },
    Release {
        coverage: Wgs84BoundingBox,
        state: DatasetReleaseState,
        record_version: u64,
        attribution: String,
    },
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MapPropertyExcerpt {
    pub name: String,
    pub value: String,
    pub truncated: bool,
}
