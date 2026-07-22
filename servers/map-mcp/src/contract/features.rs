use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use geo::{Validation, Winding};
use geo_types::{
    Coord, Geometry, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    AccessSubject, DataLabelId, DelegationId, InvocationMode, PolicyVersion, PrincipalId,
    WorkContextId,
};

use super::{
    FeatureChangeSetId, FeatureLayerId, FeatureSchemaRevisionId, LayerPublicationId, MapFeatureId,
    StyleRevisionId, Wgs84BoundingBox,
};

pub const MAX_DIRECT_FEATURE_MUTATIONS: usize = 100;
pub const MAX_DIRECT_FEATURE_BYTES: usize = 1024 * 1024;
pub const MAX_FEATURE_COORDINATES: usize = 50_000;
pub const JSON_FG_CORE_CONFORMANCE: &str = "http://www.opengis.net/spec/json-fg-1/1.0/conf/core";
pub const JSON_FG_TYPES_SCHEMAS_CONFORMANCE: &str =
    "http://www.opengis.net/spec/json-fg-1/1.0/conf/types-schemas";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeatureContentClass {
    Reference,
    NamedLocations,
    Facilities,
    Boundaries,
    NetworkCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum GeoJsonFeatureType {
    Feature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum FeatureGeometryType {
    Point,
    MultiPoint,
    LineString,
    MultiLineString,
    Polygon,
    MultiPolygon,
}

/// A GeoJSON position with longitude, latitude, and optional ellipsoidal height.
///
/// The transparent array form keeps authored features valid GeoJSON while the
/// constructor and service validators retain the WGS84 and vertical contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct GeoJsonPosition(pub Vec<f64>);

impl GeoJsonPosition {
    pub fn new(longitude_deg: f64, latitude_deg: f64, ellipsoidal_height_m: Option<f64>) -> Self {
        let mut coordinates = vec![longitude_deg, latitude_deg];
        if let Some(height) = ellipsoidal_height_m {
            coordinates.push(height);
        }
        Self(coordinates)
    }

    pub fn longitude_deg(&self) -> f64 {
        self.0[0]
    }

    pub fn latitude_deg(&self) -> f64 {
        self.0[1]
    }

    pub fn ellipsoidal_height_m(&self) -> Option<f64> {
        self.0.get(2).copied()
    }

    pub fn validate(&self) -> Result<(), FeatureValidationError> {
        if !matches!(self.0.len(), 2 | 3) {
            return Err(FeatureValidationError::new(
                "a position must contain longitude, latitude, and optional ellipsoidal height",
            ));
        }
        if self.0.iter().any(|value| !value.is_finite()) {
            return Err(FeatureValidationError::new(
                "feature coordinates must be finite",
            ));
        }
        if !(-180.0..=180.0).contains(&self.longitude_deg()) {
            return Err(FeatureValidationError::new(
                "feature longitude must be within [-180, 180]",
            ));
        }
        if !(-90.0..=90.0).contains(&self.latitude_deg()) {
            return Err(FeatureValidationError::new(
                "feature latitude must be within [-90, 90]",
            ));
        }
        Ok(())
    }

    fn coord(&self) -> Coord<f64> {
        Coord {
            x: self.longitude_deg(),
            y: self.latitude_deg(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", content = "coordinates")]
pub enum FeatureGeometry {
    Point(GeoJsonPosition),
    MultiPoint(Vec<GeoJsonPosition>),
    LineString(Vec<GeoJsonPosition>),
    MultiLineString(Vec<Vec<GeoJsonPosition>>),
    Polygon(Vec<Vec<GeoJsonPosition>>),
    MultiPolygon(Vec<Vec<Vec<GeoJsonPosition>>>),
}

impl FeatureGeometry {
    pub fn geometry_type(&self) -> FeatureGeometryType {
        match self {
            Self::Point(_) => FeatureGeometryType::Point,
            Self::MultiPoint(_) => FeatureGeometryType::MultiPoint,
            Self::LineString(_) => FeatureGeometryType::LineString,
            Self::MultiLineString(_) => FeatureGeometryType::MultiLineString,
            Self::Polygon(_) => FeatureGeometryType::Polygon,
            Self::MultiPolygon(_) => FeatureGeometryType::MultiPolygon,
        }
    }

    pub fn coordinate_count(&self) -> usize {
        match self {
            Self::Point(_) => 1,
            Self::MultiPoint(points) | Self::LineString(points) => points.len(),
            Self::MultiLineString(lines) | Self::Polygon(lines) => lines.iter().map(Vec::len).sum(),
            Self::MultiPolygon(polygons) => polygons
                .iter()
                .flat_map(|polygon| polygon.iter())
                .map(Vec::len)
                .sum(),
        }
    }

    pub fn validate(&self) -> Result<(), FeatureValidationError> {
        let coordinate_count = self.coordinate_count();
        if coordinate_count == 0 || coordinate_count > MAX_FEATURE_COORDINATES {
            return Err(FeatureValidationError::new(
                "feature coordinate count must be within 1..=50000",
            ));
        }
        for position in self.positions() {
            position.validate()?;
        }
        match self {
            Self::Point(_) => {}
            Self::MultiPoint(points) if points.is_empty() => {
                return Err(FeatureValidationError::new(
                    "a MultiPoint requires at least one position",
                ));
            }
            Self::LineString(points) => validate_line(points)?,
            Self::MultiLineString(lines) => {
                if lines.is_empty() {
                    return Err(FeatureValidationError::new(
                        "a MultiLineString requires at least one line",
                    ));
                }
                lines.iter().try_for_each(|line| validate_line(line))?;
            }
            Self::Polygon(rings) => validate_polygon(rings)?,
            Self::MultiPolygon(polygons) => {
                if polygons.is_empty() {
                    return Err(FeatureValidationError::new(
                        "a MultiPolygon requires at least one polygon",
                    ));
                }
                polygons
                    .iter()
                    .try_for_each(|polygon| validate_polygon(polygon))?;
            }
            Self::MultiPoint(_) => {}
        }
        self.to_geo()
            .check_validation()
            .map_err(|error| FeatureValidationError::owned(error.to_string()))
    }

    pub fn bounding_box(&self) -> Wgs84BoundingBox {
        let positions = self.positions().collect::<Vec<_>>();
        let south = positions
            .iter()
            .map(|position| position.latitude_deg())
            .fold(f64::INFINITY, f64::min);
        let north = positions
            .iter()
            .map(|position| position.latitude_deg())
            .fold(f64::NEG_INFINITY, f64::max);
        let mut longitudes = positions
            .iter()
            .map(|position| position.longitude_deg())
            .collect::<Vec<_>>();
        longitudes.sort_by(f64::total_cmp);
        let (west, east) = minimal_longitude_arc(&longitudes);
        Wgs84BoundingBox {
            west,
            south,
            east,
            north,
        }
    }

    pub fn to_geojson_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    fn positions(&self) -> Box<dyn Iterator<Item = &GeoJsonPosition> + '_> {
        match self {
            Self::Point(point) => Box::new(std::iter::once(point)),
            Self::MultiPoint(points) | Self::LineString(points) => Box::new(points.iter()),
            Self::MultiLineString(lines) | Self::Polygon(lines) => Box::new(lines.iter().flatten()),
            Self::MultiPolygon(polygons) => Box::new(polygons.iter().flatten().flatten()),
        }
    }

    fn to_geo(&self) -> Geometry<f64> {
        match self {
            Self::Point(point) => {
                Geometry::Point(Point::new(point.longitude_deg(), point.latitude_deg()))
            }
            Self::MultiPoint(points) => Geometry::MultiPoint(MultiPoint::new(
                points
                    .iter()
                    .map(|point| Point::new(point.longitude_deg(), point.latitude_deg()))
                    .collect(),
            )),
            Self::LineString(points) => Geometry::LineString(line_string(points)),
            Self::MultiLineString(lines) => Geometry::MultiLineString(MultiLineString::new(
                lines.iter().map(|line| line_string(line)).collect(),
            )),
            Self::Polygon(rings) => Geometry::Polygon(polygon(rings)),
            Self::MultiPolygon(polygons) => Geometry::MultiPolygon(MultiPolygon::new(
                polygons.iter().map(|rings| polygon(rings)).collect(),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureValidationError(String);

impl FeatureValidationError {
    fn new(message: &'static str) -> Self {
        Self(message.to_owned())
    }

    fn owned(message: String) -> Self {
        Self(message)
    }
}

impl std::fmt::Display for FeatureValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for FeatureValidationError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct JsonFgTimeBoundary(String);

impl JsonFgTimeBoundary {
    pub fn open() -> Self {
        Self("..".to_owned())
    }

    pub fn timestamp(value: DateTime<Utc>) -> Self {
        Self(value.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true))
    }

    pub fn as_timestamp(&self) -> Option<DateTime<Utc>> {
        (self.0 != "..").then(|| {
            DateTime::parse_from_rfc3339(&self.0)
                .expect("JsonFgTimeBoundary is validated at construction")
                .with_timezone(&Utc)
        })
    }
}

impl TryFrom<String> for JsonFgTimeBoundary {
    type Error = FeatureValidationError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value == ".." || DateTime::parse_from_rfc3339(&value).is_ok() {
            Ok(Self(value))
        } else {
            Err(FeatureValidationError::new(
                "a JSON-FG time boundary must be an RFC 3339 timestamp or `..`",
            ))
        }
    }
}

impl From<JsonFgTimeBoundary> for String {
    fn from(value: JsonFgTimeBoundary) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FeatureTime {
    /// Inclusive JSON-FG valid-time interval. The `..` string is an open bound.
    pub interval: [JsonFgTimeBoundary; 2],
}

impl FeatureTime {
    pub fn validate(&self) -> Result<(), FeatureValidationError> {
        if let [Some(start), Some(end)] = [
            self.interval[0].as_timestamp(),
            self.interval[1].as_timestamp(),
        ] && start > end
        {
            return Err(FeatureValidationError::new(
                "feature valid-time start must not exceed end",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FeatureProvenance {
    pub actor_id: PrincipalId,
    pub work_context: WorkContextId,
    pub policy_revision: PolicyVersion,
    pub invocation_mode: InvocationMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initiator_id: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_id: Option<DelegationId>,
}

/// Canonical authored map feature. Its core fields remain valid GeoJSON and
/// its temporal and feature-type members follow JSON-FG vocabulary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MapFeature {
    #[serde(rename = "type")]
    pub feature_type: GeoJsonFeatureType,
    #[serde(rename = "conformsTo")]
    pub conforms_to: Vec<String>,
    pub id: MapFeatureId,
    pub geometry: FeatureGeometry,
    #[serde(default)]
    pub properties: BTreeMap<String, serde_json::Value>,
    #[serde(rename = "featureType")]
    pub semantic_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<FeatureTime>,
    pub layer_id: FeatureLayerId,
    pub feature_revision: u64,
    pub layer_revision: u64,
    pub schema_version: u64,
    pub deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_resources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_resources: Vec<String>,
    pub provenance: FeatureProvenance,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FeatureSchemaRevision {
    pub schema_revision_id: FeatureSchemaRevisionId,
    pub layer_id: FeatureLayerId,
    pub version: u64,
    pub digest_sha256: String,
    pub schema: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerStyle {
    #[serde(default)]
    pub rules: Vec<StyleRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StyleRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry_type: Option<FeatureGeometryType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_zoom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_zoom: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_opacity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_width_px: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circle_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub circle_radius_px: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_property: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MapStyleRevision {
    pub style_revision_id: StyleRevisionId,
    pub layer_id: FeatureLayerId,
    pub version: u64,
    pub style: LayerStyle,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FeatureLayer {
    pub layer_id: FeatureLayerId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub content_class: FeatureContentClass,
    pub schema: FeatureSchemaRevision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<MapStyleRevision>,
    pub revision: u64,
    pub owner: AccessSubject,
    pub created_by: PrincipalId,
    pub work_context: WorkContextId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<DataLabelId>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub data_labels: std::collections::BTreeSet<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateFeatureLayerRequest {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub content_class: FeatureContentClass,
    pub property_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<LayerStyle>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateFeatureLayerRequest {
    pub layer_id: FeatureLayerId,
    pub expected_layer_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_schema: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<LayerStyle>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FeatureInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_id: Option<MapFeatureId>,
    pub geometry: FeatureGeometry,
    #[serde(default)]
    pub properties: BTreeMap<String, serde_json::Value>,
    pub semantic_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<FeatureTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_resources: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_resources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FeatureMutation {
    Create {
        feature: FeatureInput,
    },
    Replace {
        feature_id: MapFeatureId,
        expected_feature_revision: u64,
        feature: FeatureInput,
    },
    Tombstone {
        feature_id: MapFeatureId,
        expected_feature_revision: u64,
    },
    Restore {
        feature_id: MapFeatureId,
        expected_feature_revision: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValidateFeatureChangesRequest {
    pub layer_id: FeatureLayerId,
    pub expected_layer_revision: u64,
    pub mutations: Vec<FeatureMutation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommitFeatureChangesRequest {
    pub layer_id: FeatureLayerId,
    pub expected_layer_revision: u64,
    pub idempotency_key: String,
    pub mutations: Vec<FeatureMutation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FeatureValidationFinding {
    pub mutation_index: usize,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ValidateFeatureChangesOutput {
    pub valid: bool,
    pub layer_id: FeatureLayerId,
    pub expected_layer_revision: u64,
    pub findings: Vec<FeatureValidationFinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionState {
    Ready,
    Pending,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FeatureChangeSet {
    pub changeset_id: FeatureChangeSetId,
    pub layer_id: FeatureLayerId,
    pub base_layer_revision: u64,
    pub resulting_layer_revision: u64,
    pub feature_ids: Vec<MapFeatureId>,
    pub idempotency_key: String,
    pub request_digest_sha256: String,
    pub actor_id: PrincipalId,
    pub work_context: WorkContextId,
    pub commit_sequence: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CommitFeatureChangesOutput {
    pub changeset: FeatureChangeSet,
    pub features: Vec<MapFeature>,
    pub projection_state: ProjectionState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RestoreFeatureRequest {
    pub layer_id: FeatureLayerId,
    pub feature_id: MapFeatureId,
    pub expected_layer_revision: u64,
    pub expected_feature_revision: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Cql2Operator {
    #[serde(rename = "and")]
    And,
    #[serde(rename = "or")]
    Or,
    #[serde(rename = "not")]
    Not,
    #[serde(rename = "=")]
    Equal,
    #[serde(rename = "<>")]
    NotEqual,
    #[serde(rename = "<")]
    LessThan,
    #[serde(rename = "<=")]
    LessThanOrEqual,
    #[serde(rename = ">")]
    GreaterThan,
    #[serde(rename = ">=")]
    GreaterThanOrEqual,
    #[serde(rename = "isNull")]
    IsNull,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cql2Filter {
    pub op: Cql2Operator,
    pub args: Vec<Cql2Expression>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Cql2Expression {
    Operation(Box<Cql2Filter>),
    Property(Cql2PropertyReference),
    Literal(Cql2Literal),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Cql2PropertyReference {
    pub property: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Cql2Literal {
    String(String),
    Number(f64),
    Boolean(bool),
    Null(()),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QueryFeaturesRequest {
    pub layer_id: FeatureLayerId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication_id: Option<LayerPublicationId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<Wgs84BoundingBox>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datetime: Option<FeatureTime>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry_type: Option<FeatureGeometryType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<Cql2Filter>,
    #[serde(default = "default_feature_query_limit")]
    pub limit: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_commit_sequence: Option<u64>,
}

fn default_feature_query_limit() -> u32 {
    100
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QueryFeaturesOutput {
    pub layer_id: FeatureLayerId,
    pub features: Vec<MapFeature>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub projection_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PublishFeatureLayerRequest {
    pub layer_id: FeatureLayerId,
    pub expected_layer_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LayerPublication {
    pub publication_id: LayerPublicationId,
    pub layer_id: FeatureLayerId,
    pub layer_revision: u64,
    pub schema_version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_revision_id: Option<StyleRevisionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_uris: Vec<String>,
    pub published_by: PrincipalId,
    pub work_context: WorkContextId,
    pub published_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArchiveFeatureLayerRequest {
    pub layer_id: FeatureLayerId,
    pub expected_layer_revision: u64,
}

fn validate_line(line: &[GeoJsonPosition]) -> Result<(), FeatureValidationError> {
    if line.len() < 2 {
        return Err(FeatureValidationError::new(
            "a LineString requires at least two positions",
        ));
    }
    Ok(())
}

fn validate_polygon(rings: &[Vec<GeoJsonPosition>]) -> Result<(), FeatureValidationError> {
    let Some(exterior) = rings.first() else {
        return Err(FeatureValidationError::new(
            "a Polygon requires an exterior ring",
        ));
    };
    validate_ring(exterior)?;
    for interior in &rings[1..] {
        validate_ring(interior)?;
    }
    let exterior = line_string(exterior);
    if !exterior.is_ccw() {
        return Err(FeatureValidationError::new(
            "a Polygon exterior ring must use counter-clockwise winding",
        ));
    }
    for interior in &rings[1..] {
        if line_string(interior).is_ccw() {
            return Err(FeatureValidationError::new(
                "a Polygon interior ring must use clockwise winding",
            ));
        }
    }
    Ok(())
}

fn validate_ring(ring: &[GeoJsonPosition]) -> Result<(), FeatureValidationError> {
    if ring.len() < 4 {
        return Err(FeatureValidationError::new(
            "a Polygon ring requires at least four positions",
        ));
    }
    if ring.first() != ring.last() {
        return Err(FeatureValidationError::new("a Polygon ring must be closed"));
    }
    Ok(())
}

fn line_string(points: &[GeoJsonPosition]) -> LineString<f64> {
    LineString::new(points.iter().map(GeoJsonPosition::coord).collect())
}

fn polygon(rings: &[Vec<GeoJsonPosition>]) -> Polygon<f64> {
    Polygon::new(
        line_string(&rings[0]),
        rings[1..].iter().map(|ring| line_string(ring)).collect(),
    )
}

fn minimal_longitude_arc(longitudes: &[f64]) -> (f64, f64) {
    if longitudes.len() <= 1 {
        let value = longitudes.first().copied().unwrap_or(0.0);
        return (value, value);
    }
    let mut largest_gap = f64::NEG_INFINITY;
    let mut largest_gap_index = 0;
    for index in 0..longitudes.len() {
        let current = longitudes[index];
        let next = if index + 1 == longitudes.len() {
            longitudes[0] + 360.0
        } else {
            longitudes[index + 1]
        };
        let gap = next - current;
        if gap > largest_gap {
            largest_gap = gap;
            largest_gap_index = index;
        }
    }
    let west = longitudes[(largest_gap_index + 1) % longitudes.len()];
    let east = longitudes[largest_gap_index];
    (west, east)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(longitude: f64, latitude: f64) -> GeoJsonPosition {
        GeoJsonPosition::new(longitude, latitude, None)
    }

    #[test]
    fn geometry_serializes_as_geojson() {
        let geometry = FeatureGeometry::Point(position(-89.2, 13.7));
        assert_eq!(
            serde_json::to_value(geometry).unwrap(),
            serde_json::json!({"type": "Point", "coordinates": [-89.2, 13.7]})
        );
    }

    #[test]
    fn polygons_require_topology_and_right_hand_winding() {
        let valid = FeatureGeometry::Polygon(vec![vec![
            position(0.0, 0.0),
            position(1.0, 0.0),
            position(1.0, 1.0),
            position(0.0, 1.0),
            position(0.0, 0.0),
        ]]);
        assert!(valid.validate().is_ok());

        let bow_tie = FeatureGeometry::Polygon(vec![vec![
            position(0.0, 0.0),
            position(1.0, 1.0),
            position(1.0, 0.0),
            position(0.0, 1.0),
            position(0.0, 0.0),
        ]]);
        assert!(bow_tie.validate().is_err());
    }

    #[test]
    fn bounding_box_preserves_dateline_crossing() {
        let geometry =
            FeatureGeometry::LineString(vec![position(179.0, 1.0), position(-179.0, 2.0)]);
        let bbox = geometry.bounding_box();
        assert_eq!((bbox.west, bbox.east), (179.0, -179.0));
    }

    #[test]
    fn json_fg_time_uses_the_standard_open_interval_marker() {
        let time = FeatureTime {
            interval: [
                JsonFgTimeBoundary::open(),
                JsonFgTimeBoundary::timestamp(
                    DateTime::parse_from_rfc3339("2026-07-22T12:00:00Z")
                        .unwrap()
                        .with_timezone(&Utc),
                ),
            ],
        };

        assert_eq!(
            serde_json::to_value(&time).unwrap(),
            serde_json::json!({"interval": ["..", "2026-07-22T12:00:00Z"]})
        );
        assert!(serde_json::from_value::<FeatureTime>(serde_json::json!({
            "interval": [null, "2026-07-22T12:00:00Z"]
        }))
        .is_err());
    }
}
