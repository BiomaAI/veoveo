use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::ArtifactId;

use super::{FeatureChangeSet, FeatureLayerId, LayerProduct, LayerPublicationId, ProjectionState};

pub const MAX_IMPORT_FEATURES: usize = 10_000;
pub const MAX_VECTOR_TILES: usize = 512;
pub const MAX_VECTOR_TILE_ZOOM: u8 = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeatureImportFormat {
    GeoJsonFeatureCollection,
    GeoJsonTextSequence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImportFeatureLayerRequest {
    pub layer_id: FeatureLayerId,
    pub expected_layer_revision: u64,
    pub source_artifact_id: ArtifactId,
    pub format: FeatureImportFormat,
    /// Used when an input GeoJSON feature omits JSON-FG `featureType`.
    pub default_semantic_type: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImportFeatureLayerOutput {
    pub imported_feature_count: u64,
    pub changeset: FeatureChangeSet,
    pub projection_state: ProjectionState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeatureExportFormat {
    GeoJsonSeq,
    GeoParquet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ExportFeatureLayerRequest {
    pub layer_id: FeatureLayerId,
    pub publication_id: LayerPublicationId,
    pub format: FeatureExportFormat,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExportFeatureLayerOutput {
    pub product: LayerProduct,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub struct TileCoordinate {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

impl TileCoordinate {
    pub fn validate(&self) -> Result<(), String> {
        if self.z > MAX_VECTOR_TILE_ZOOM {
            return Err(format!(
                "vector tile zoom cannot exceed {MAX_VECTOR_TILE_ZOOM}"
            ));
        }
        let width = 1_u32 << self.z;
        if self.x >= width || self.y >= width {
            return Err("vector tile x and y must be within the zoom pyramid".to_owned());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BuildVectorTilesRequest {
    pub layer_id: FeatureLayerId,
    pub publication_id: LayerPublicationId,
    pub tiles: Vec<TileCoordinate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BuildVectorTilesOutput {
    pub product: LayerProduct,
    pub tile_count: u64,
}
