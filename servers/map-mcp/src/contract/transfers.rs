use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::ArtifactId;

use super::{
    FeatureChangeSet, FeatureLayerId, LayerProduct, LayerPublicationId, ProjectionState,
    Wgs84BoundingBox,
};

pub const MAX_IMPORT_FEATURES: usize = 10_000;
pub const MAX_VECTOR_TILES: usize = 512;
pub const MAX_VECTOR_TILE_ZOOM: u8 = 22;

pub const MAX_GEOPACKAGE_TABLES: usize = 256;
pub const MAX_GEOPACKAGE_FIELDS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct GeoPackageIdentifier(String);

impl GeoPackageIdentifier {
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
            return Err(
                "GeoPackage identifiers must contain 1 to 255 non-control UTF-8 characters"
                    .to_owned(),
            );
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for GeoPackageIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum FeatureImportSource {
    GeoJsonFeatureCollection {
        /// Used when an input feature omits JSON-FG `featureType`.
        default_semantic_type: String,
    },
    GeoJsonTextSequence {
        /// Used when an input feature omits JSON-FG `featureType`.
        default_semantic_type: String,
    },
    GeoPackage {
        table: GeoPackageIdentifier,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        identity_column: Option<GeoPackageIdentifier>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        semantic_type_column: Option<GeoPackageIdentifier>,
        default_semantic_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title_column: Option<GeoPackageIdentifier>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_from_column: Option<GeoPackageIdentifier>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        valid_until_column: Option<GeoPackageIdentifier>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ImportFeatureLayerRequest {
    pub layer_id: FeatureLayerId,
    pub expected_layer_revision: u64,
    pub source_artifact_id: ArtifactId,
    pub source: FeatureImportSource,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ImportFeatureLayerOutput {
    pub imported_feature_count: u64,
    pub changeset: FeatureChangeSet,
    pub projection_state: ProjectionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "format", rename_all = "snake_case")]
pub enum FeatureExportFormat {
    GeoJsonSeq,
    GeoParquet,
    GeoPackage { table: GeoPackageIdentifier },
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InspectGeoPackageRequest {
    pub source_artifact_id: ArtifactId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GeoPackageFindingLevel {
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GeoPackageFinding {
    pub level: GeoPackageFindingLevel,
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<GeoPackageIdentifier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GeoPackageFieldType {
    Boolean,
    Integer,
    Integer64,
    Real,
    String,
    Binary,
    Date,
    DateTime,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GeoPackageField {
    pub name: GeoPackageIdentifier,
    pub field_type: GeoPackageFieldType,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeoPackageFeatureTable {
    pub table: GeoPackageIdentifier,
    pub identifier: String,
    pub description: String,
    pub geometry_column: GeoPackageIdentifier,
    pub geometry_type: String,
    pub srs_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crs_name: Option<String>,
    pub feature_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extent_wgs84: Option<Wgs84BoundingBox>,
    pub fields: Vec<GeoPackageField>,
    pub has_spatial_index: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GeoPackageExtension {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<GeoPackageIdentifier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<GeoPackageIdentifier>,
    pub definition: String,
    pub scope: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeoPackageManifest {
    pub version: String,
    pub application_id: u32,
    pub user_version: u32,
    pub feature_tables: Vec<GeoPackageFeatureTable>,
    pub extensions: Vec<GeoPackageExtension>,
    pub findings: Vec<GeoPackageFinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InspectGeoPackageOutput {
    pub manifest: GeoPackageManifest,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geopackage_identifier_is_bounded_but_not_sql_shaped() {
        assert_eq!(
            GeoPackageIdentifier::new("Named places 2026")
                .expect("valid identifier")
                .as_str(),
            "Named places 2026"
        );
        assert!(GeoPackageIdentifier::new("").is_err());
        assert!(GeoPackageIdentifier::new("bad\nname").is_err());
        assert!(GeoPackageIdentifier::new("x".repeat(256)).is_err());
    }

    #[test]
    fn geopackage_import_requires_explicit_table_and_mapping() {
        let value = serde_json::json!({
            "layer_id": "feature-layer-019c0000-0000-7000-8000-000000000001",
            "expected_layer_revision": 3,
            "source_artifact_id": "019c0000-0000-7000-8000-000000000002",
            "source": {
                "format": "geo_package",
                "table": "named places",
                "identity_column": "external id",
                "default_semantic_type": "NamedPlace"
            },
            "idempotency_key": "import-019c0000-0000-7000-8000-000000000003"
        });
        let request: ImportFeatureLayerRequest =
            serde_json::from_value(value).expect("typed GeoPackage import");
        assert!(matches!(
            request.source,
            FeatureImportSource::GeoPackage { .. }
        ));
    }
}
