use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veoveo_mcp_contract::{PrincipalId, WorkContextId, parse_artifact_plane_uri};

use super::{
    DatasetLicense, DatasetReleaseId, FeatureGeometry, MapSourceId, Meters, RasterDerivationId,
    RasterProductId, SourceFeatureId, Wgs84BoundingBox, Wgs84Position,
};

pub const SOURCE_FEATURE_SCHEMA_VERSION: u64 = 1;
pub const RASTER_PRODUCT_SCHEMA_VERSION: u64 = 1;
pub const RASTER_DERIVATION_SCHEMA_VERSION: u64 = 1;
pub const MAX_SOURCE_QUERY_LIMIT: u32 = 500;
pub const MAX_SOURCE_TAG_FILTERS: usize = 32;
pub const MAX_RASTER_WINDOW_PIXELS: u64 = 16_777_216;
pub const MAX_RASTER_SAMPLE_POSITIONS: usize = 10_000;
pub const MAX_RASTER_FULL_DERIVATION_PIXELS: u64 = 4_194_304;
pub const RASTER_DERIVATION_ALGORITHM_REVISION: &str = "gdal-3.13.2-veoveo-raster-v1";

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceElementType {
    Node,
    Way,
    Relation,
    Feature,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SourceFeatureRepresentation {
    Point,
    Line,
    Polygon,
    Relation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceFeature {
    pub schema_version: u64,
    pub feature_id: SourceFeatureId,
    pub source_id: MapSourceId,
    pub release_id: DatasetReleaseId,
    pub source_element_type: SourceElementType,
    pub source_element_id: String,
    pub source_element_version: String,
    pub representation: SourceFeatureRepresentation,
    pub geometry: FeatureGeometry,
    pub geometry_digest_sha256: String,
    /// Source-owned properties are deliberately open-ended. Map preserves
    /// every normalized key and value rather than projecting an allowlist.
    pub normalized_tags: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub original_names: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub original_references: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub operating_area_ids: BTreeSet<String>,
    pub source_digest_sha256: String,
    pub license: DatasetLicense,
    pub acquired_at: DateTime<Utc>,
}

impl SourceFeature {
    pub fn validate(&self) -> Result<(), SourceProductError> {
        if self.schema_version != SOURCE_FEATURE_SCHEMA_VERSION {
            return Err(SourceProductError::UnsupportedSchema);
        }
        validate_controlled(&self.source_element_id, 512)?;
        validate_controlled(&self.source_element_version, 256)?;
        validate_sha256(&self.geometry_digest_sha256)?;
        validate_sha256(&self.source_digest_sha256)?;
        self.geometry
            .validate()
            .map_err(|_| SourceProductError::InvalidGeometry)?;
        if representation_for_geometry(&self.geometry) != self.representation
            && self.representation != SourceFeatureRepresentation::Relation
        {
            return Err(SourceProductError::RepresentationMismatch);
        }
        if self.normalized_tags.len() > 4_096 {
            return Err(SourceProductError::TooManyTags);
        }
        for (key, value) in &self.normalized_tags {
            validate_controlled(key, 512)?;
            if serde_json::to_vec(value)
                .map_err(|_| SourceProductError::InvalidTag)?
                .len()
                > 16_384
            {
                return Err(SourceProductError::InvalidTag);
            }
        }
        for (key, value) in &self.original_names {
            validate_controlled(key, 64)?;
            validate_controlled(value, 1_024)?;
        }
        for value in self
            .original_references
            .iter()
            .chain(self.operating_area_ids.iter())
        {
            validate_controlled(value, 512)?;
        }
        self.license
            .validate()
            .map_err(|_| SourceProductError::InvalidLicense)
    }
}

pub fn representation_for_geometry(geometry: &FeatureGeometry) -> SourceFeatureRepresentation {
    match geometry {
        FeatureGeometry::Point(_) | FeatureGeometry::MultiPoint(_) => {
            SourceFeatureRepresentation::Point
        }
        FeatureGeometry::LineString(_) | FeatureGeometry::MultiLineString(_) => {
            SourceFeatureRepresentation::Line
        }
        FeatureGeometry::Polygon(_) | FeatureGeometry::MultiPolygon(_) => {
            SourceFeatureRepresentation::Polygon
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceTagEquality {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceSpatialQuery {
    BoundingBox {
        bounds: Wgs84BoundingBox,
    },
    Intersects {
        geometry: FeatureGeometry,
    },
    Contains {
        geometry: FeatureGeometry,
    },
    Within {
        geometry: FeatureGeometry,
    },
    WithinDistance {
        position: Wgs84Position,
        distance: Meters,
    },
    Nearest {
        position: Wgs84Position,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        maximum_distance: Option<Meters>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuerySourceFeaturesRequest {
    /// Source feature queries always select one immutable release.
    pub release_id: DatasetReleaseId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<MapSourceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_element_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub representation: Option<SourceFeatureRepresentation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags_equal: Vec<SourceTagEquality>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags_exist: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normalized_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spatial: Option<SourceSpatialQuery>,
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl QuerySourceFeaturesRequest {
    pub fn validate(&self) -> Result<(), SourceProductError> {
        if !(1..=MAX_SOURCE_QUERY_LIMIT).contains(&self.limit) {
            return Err(SourceProductError::InvalidLimit);
        }
        if self.tags_equal.len() + self.tags_exist.len() > MAX_SOURCE_TAG_FILTERS {
            return Err(SourceProductError::TooManyTagFilters);
        }
        if let Some(value) = self.source_element_id.as_deref() {
            validate_controlled(value, 512)?;
        }
        if let Some(value) = self.normalized_text.as_deref() {
            validate_controlled(value, 256)?;
        }
        for filter in &self.tags_equal {
            validate_controlled(&filter.key, 512)?;
            validate_controlled(&filter.value, 16_384)?;
        }
        for key in &self.tags_exist {
            validate_controlled(key, 512)?;
        }
        if let Some(spatial) = &self.spatial {
            match spatial {
                SourceSpatialQuery::BoundingBox { bounds } => bounds
                    .validate()
                    .map_err(|_| SourceProductError::InvalidGeometry)?,
                SourceSpatialQuery::Intersects { geometry }
                | SourceSpatialQuery::Contains { geometry }
                | SourceSpatialQuery::Within { geometry } => geometry
                    .validate()
                    .map_err(|_| SourceProductError::InvalidGeometry)?,
                SourceSpatialQuery::WithinDistance { position, distance } => {
                    position
                        .validate()
                        .map_err(|_| SourceProductError::InvalidGeometry)?;
                    validate_distance(*distance)?;
                }
                SourceSpatialQuery::Nearest {
                    position,
                    maximum_distance,
                } => {
                    position
                        .validate()
                        .map_err(|_| SourceProductError::InvalidGeometry)?;
                    if let Some(distance) = maximum_distance {
                        validate_distance(*distance)?;
                    }
                }
            }
        }
        if self
            .cursor
            .as_ref()
            .is_some_and(|value| value.len() > 2_048)
        {
            return Err(SourceProductError::InvalidCursor);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceFeatureMatch {
    pub feature: SourceFeature,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<Meters>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuerySourceFeaturesOutput {
    pub release_id: DatasetReleaseId,
    pub query_digest_sha256: String,
    pub features: Vec<SourceFeatureMatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RasterValueInterpretation {
    Continuous,
    Categorical,
    Probability,
    VectorComponent,
    Color,
    Mask,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RasterBand {
    pub index: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub data_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    pub interpretation: RasterValueInterpretation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodata: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RasterProduct {
    pub schema_version: u64,
    pub raster_id: RasterProductId,
    pub source_id: MapSourceId,
    pub release_id: DatasetReleaseId,
    pub artifact_uri: String,
    pub checksum_sha256: String,
    pub crs: String,
    /// GDAL geotransform `[origin_x, pixel_width, row_rotation,
    /// origin_y, column_rotation, pixel_height]`.
    pub transform: [f64; 6],
    pub width: u32,
    pub height: u32,
    pub extent: [f64; 4],
    pub resolution: [f64; 2],
    pub bands: Vec<RasterBand>,
    pub license: DatasetLicense,
    pub attribution: String,
}

impl RasterProduct {
    pub fn validate(&self) -> Result<(), SourceProductError> {
        if self.schema_version != RASTER_PRODUCT_SCHEMA_VERSION
            || self.width == 0
            || self.height == 0
            || self.bands.is_empty()
            || self.bands.len() > 256
            || self.transform.iter().any(|value| !value.is_finite())
            || self.extent.iter().any(|value| !value.is_finite())
            || self.extent[0] >= self.extent[2]
            || self.extent[1] >= self.extent[3]
            || self.resolution.iter().any(|value| !value.is_finite())
            || self.resolution.iter().any(|value| *value <= 0.0)
            || !valid_transform(&self.transform)
            || !self.artifact_uri.starts_with("artifact://")
            || parse_artifact_plane_uri(&self.artifact_uri).is_none()
        {
            return Err(SourceProductError::InvalidRaster);
        }
        validate_controlled(&self.crs, 16_384)?;
        validate_text(&self.attribution, 4_096)?;
        validate_sha256(&self.checksum_sha256)?;
        self.license
            .validate()
            .map_err(|_| SourceProductError::InvalidLicense)?;
        for (offset, band) in self.bands.iter().enumerate() {
            if band.index != u32::try_from(offset + 1).unwrap_or(u32::MAX)
                || band.nodata.is_some_and(|value| !value.is_finite())
            {
                return Err(SourceProductError::InvalidRaster);
            }
            validate_controlled(&band.data_type, 64)?;
            if let Some(value) = band.name.as_deref() {
                validate_controlled(value, 256)?;
            }
            if let Some(value) = band.unit.as_deref() {
                validate_controlled(value, 128)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RasterDerivationOperation {
    Sample {
        band: u32,
        positions: Vec<Wgs84Position>,
    },
    Window {
        bounds: Wgs84BoundingBox,
        width: u32,
        height: u32,
    },
    ClassMask {
        band: u32,
        classes: BTreeSet<i64>,
    },
    Contour {
        band: u32,
        interval: f64,
        base: f64,
    },
    Polygonize {
        band: u32,
    },
    Skeletonize {
        band: u32,
        threshold: f64,
    },
    DeriveLines {
        band: u32,
        threshold: f64,
        minimum_length: Meters,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeriveRasterRequest {
    pub raster_id: RasterProductId,
    pub operation: RasterDerivationOperation,
    pub algorithm_revision: String,
}

impl DeriveRasterRequest {
    pub fn validate(&self) -> Result<(), SourceProductError> {
        validate_controlled(&self.algorithm_revision, 256)?;
        if self.algorithm_revision != RASTER_DERIVATION_ALGORITHM_REVISION {
            return Err(SourceProductError::UnsupportedRasterAlgorithm);
        }
        match &self.operation {
            RasterDerivationOperation::Sample { band, positions } => {
                validate_band(*band)?;
                if positions.is_empty() || positions.len() > MAX_RASTER_SAMPLE_POSITIONS {
                    return Err(SourceProductError::InvalidRasterOperation);
                }
                positions.iter().try_for_each(|position| {
                    position
                        .validate()
                        .map_err(|_| SourceProductError::InvalidRasterOperation)
                })?;
            }
            RasterDerivationOperation::Window {
                bounds,
                width,
                height,
            } => {
                bounds
                    .validate()
                    .map_err(|_| SourceProductError::InvalidRasterOperation)?;
                if bounds.west > bounds.east
                    || *width == 0
                    || *height == 0
                    || u64::from(*width) * u64::from(*height) > MAX_RASTER_WINDOW_PIXELS
                {
                    return Err(SourceProductError::InvalidRasterOperation);
                }
            }
            RasterDerivationOperation::ClassMask { band, classes } => {
                validate_band(*band)?;
                if classes.is_empty() || classes.len() > 256 {
                    return Err(SourceProductError::InvalidRasterOperation);
                }
            }
            RasterDerivationOperation::Contour {
                band,
                interval,
                base,
            } => {
                validate_band(*band)?;
                if !interval.is_finite() || *interval <= 0.0 || !base.is_finite() {
                    return Err(SourceProductError::InvalidRasterOperation);
                }
            }
            RasterDerivationOperation::Polygonize { band } => validate_band(*band)?,
            RasterDerivationOperation::Skeletonize { band, threshold } => {
                validate_band(*band)?;
                if !threshold.is_finite() {
                    return Err(SourceProductError::InvalidRasterOperation);
                }
            }
            RasterDerivationOperation::DeriveLines {
                band,
                threshold,
                minimum_length,
            } => {
                validate_band(*band)?;
                if !threshold.is_finite()
                    || !minimum_length.get().is_finite()
                    || minimum_length.get() < 0.0
                {
                    return Err(SourceProductError::InvalidRasterOperation);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RasterDerivation {
    pub schema_version: u64,
    pub derivation_id: RasterDerivationId,
    pub source_raster_id: RasterProductId,
    pub source_release_id: DatasetReleaseId,
    pub source_checksum_sha256: String,
    pub source_crs: String,
    pub source_transform: [f64; 6],
    pub operation: RasterDerivationOperation,
    pub algorithm_revision: String,
    pub output_artifact_uri: String,
    pub output_mime_type: String,
    pub output_crs: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_transform: Option<[f64; 6]>,
    pub output_checksum_sha256: String,
    pub created_by: PrincipalId,
    pub work_context: WorkContextId,
    pub created_at: DateTime<Utc>,
}

impl RasterDerivation {
    pub fn validate(&self) -> Result<(), SourceProductError> {
        if self.schema_version != RASTER_DERIVATION_SCHEMA_VERSION {
            return Err(SourceProductError::UnsupportedSchema);
        }
        validate_sha256(&self.source_checksum_sha256)?;
        validate_sha256(&self.output_checksum_sha256)?;
        validate_controlled(&self.source_crs, 16_384)?;
        validate_controlled(&self.output_crs, 16_384)?;
        validate_controlled(&self.algorithm_revision, 256)?;
        validate_controlled(&self.output_mime_type, 256)?;
        if !valid_transform(&self.source_transform)
            || self
                .output_transform
                .is_some_and(|transform| !valid_transform(&transform))
            || !self.output_artifact_uri.starts_with("artifact://")
            || parse_artifact_plane_uri(&self.output_artifact_uri).is_none()
        {
            return Err(SourceProductError::InvalidRaster);
        }
        DeriveRasterRequest {
            raster_id: self.source_raster_id.clone(),
            operation: self.operation.clone(),
            algorithm_revision: self.algorithm_revision.clone(),
        }
        .validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceProductError {
    UnsupportedSchema,
    InvalidControlledValue,
    InvalidDigest,
    InvalidGeometry,
    RepresentationMismatch,
    TooManyTags,
    InvalidTag,
    InvalidLicense,
    InvalidLimit,
    TooManyTagFilters,
    InvalidCursor,
    InvalidRaster,
    InvalidRasterOperation,
    UnsupportedRasterAlgorithm,
}

impl std::fmt::Display for SourceProductError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::UnsupportedSchema => "unsupported source-product schema version",
            Self::InvalidControlledValue => "source-product text value is invalid",
            Self::InvalidDigest => "source-product SHA-256 digest is invalid",
            Self::InvalidGeometry => "source-product geometry is invalid",
            Self::RepresentationMismatch => {
                "source feature representation does not match its geometry"
            }
            Self::TooManyTags => "source feature exceeds the normalized tag limit",
            Self::InvalidTag => "source feature tag is invalid",
            Self::InvalidLicense => "source-product license is invalid",
            Self::InvalidLimit => "source-feature query limit must be within 1..=500",
            Self::TooManyTagFilters => "source-feature query exceeds 32 tag filters",
            Self::InvalidCursor => "source-feature query cursor is invalid",
            Self::InvalidRaster => "raster product metadata is invalid",
            Self::InvalidRasterOperation => {
                "raster derivation request is invalid or exceeds its bounds"
            }
            Self::UnsupportedRasterAlgorithm => {
                "raster derivation algorithm revision is unsupported"
            }
        })
    }
}

fn validate_band(band: u32) -> Result<(), SourceProductError> {
    if band == 0 || band > 256 {
        return Err(SourceProductError::InvalidRasterOperation);
    }
    Ok(())
}

fn valid_transform(transform: &[f64; 6]) -> bool {
    transform.iter().all(|value| value.is_finite())
        && transform[1] * transform[5] - transform[2] * transform[4] != 0.0
}

impl std::error::Error for SourceProductError {}

fn validate_distance(value: Meters) -> Result<(), SourceProductError> {
    if !value.get().is_finite() || value.get() <= 0.0 || value.get() > 10_000_000.0 {
        return Err(SourceProductError::InvalidGeometry);
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), SourceProductError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SourceProductError::InvalidDigest);
    }
    Ok(())
}

fn validate_controlled(value: &str, maximum_length: usize) -> Result<(), SourceProductError> {
    if value.is_empty()
        || value.len() > maximum_length
        || value.chars().any(|character| character.is_control())
    {
        return Err(SourceProductError::InvalidControlledValue);
    }
    Ok(())
}

fn validate_text(value: &str, maximum_length: usize) -> Result<(), SourceProductError> {
    if value.is_empty()
        || value.len() > maximum_length
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(SourceProductError::InvalidControlledValue);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_queries_require_an_immutable_release_and_bounded_filters() {
        let request = QuerySourceFeaturesRequest {
            release_id: DatasetReleaseId::new(),
            source_id: None,
            source_element_id: None,
            representation: None,
            tags_equal: Vec::new(),
            tags_exist: vec!["highway".to_owned(); MAX_SOURCE_TAG_FILTERS + 1],
            normalized_text: None,
            spatial: None,
            limit: 100,
            cursor: None,
        };
        assert_eq!(
            request.validate(),
            Err(SourceProductError::TooManyTagFilters)
        );
    }

    #[test]
    fn raster_windows_have_a_hard_pixel_limit_constant() {
        assert_eq!(MAX_RASTER_WINDOW_PIXELS, 4096 * 4096);
    }

    #[test]
    fn raster_derivations_pin_the_implemented_algorithm_revision() {
        let request = DeriveRasterRequest {
            raster_id: RasterProductId::new(),
            operation: RasterDerivationOperation::Polygonize { band: 1 },
            algorithm_revision: "caller-selected-implementation".to_owned(),
        };
        assert_eq!(
            request.validate(),
            Err(SourceProductError::UnsupportedRasterAlgorithm)
        );
    }
}
