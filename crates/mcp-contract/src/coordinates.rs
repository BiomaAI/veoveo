use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ArtifactMetadata;

fn validate_coordinate_id(value: &str) -> Result<(), CoordinateIdError> {
    if value.is_empty() || value.len() > 128 {
        return Err(CoordinateIdError::new(value, "must be 1 to 128 characters"));
    }
    if value.contains("://") || value.contains('/') || value.chars().any(char::is_whitespace) {
        return Err(CoordinateIdError::new(
            value,
            "must not contain whitespace, slash, or URI separators",
        ));
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | ':'))
    {
        Ok(())
    } else {
        Err(CoordinateIdError::new(
            value,
            "must contain only ASCII letters, digits, underscore, dash, dot, or colon",
        ))
    }
}

macro_rules! coordinate_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, CoordinateIdError> {
                let value = value.into();
                validate_coordinate_id(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = CoordinateIdError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl FromStr for $name {
            type Err = CoordinateIdError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value.to_string())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateIdError {
    value: String,
    rule: &'static str,
}

impl CoordinateIdError {
    fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_string(),
            rule,
        }
    }
}

impl fmt::Display for CoordinateIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid coordinate identifier {:?}: {}",
            self.value, self.rule
        )
    }
}

impl std::error::Error for CoordinateIdError {}

coordinate_id!(
    FrameId,
    "Canonical coordinate frame id, such as WGS84, ECEF, ENU:mission-a, or robot body frame id."
);
coordinate_id!(CrsId, "Coordinate reference system id, commonly EPSG:4326.");
coordinate_id!(
    DatumId,
    "Geodetic datum id used by a CRS or coordinate operation."
);
coordinate_id!(
    EllipsoidId,
    "Reference ellipsoid id, such as WGS84 or GRS80."
);
coordinate_id!(
    CoordinateOperationId,
    "Durable id for one coordinate transform, projection, geodesic, or validation operation."
);
coordinate_id!(
    TrajectoryId,
    "Trajectory identity used by plan and robot outputs."
);
coordinate_id!(
    GeofenceId,
    "Geofence identity used by validation and plans."
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FrameKind {
    Wgs84,
    Ecef,
    Enu,
    Ned,
    Frd,
    ProjectedCrs,
    SimulationWorld,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AxisConvention {
    LatitudeLongitudeHeight,
    XyzMeters,
    EastNorthUp,
    NorthEastDown,
    ForwardRightDown,
    ProjectedXyz,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateUnit {
    Degree,
    Meter,
    Unitless,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wgs84Position {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    #[serde(default)]
    pub height_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EcefPosition {
    pub x_m: f64,
    pub y_m: f64,
    pub z_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EnuPosition {
    pub frame_id: FrameId,
    pub east_m: f64,
    pub north_m: f64,
    pub up_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NedPosition {
    pub frame_id: FrameId,
    pub north_m: f64,
    pub east_m: f64,
    pub down_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectedPosition {
    pub crs: CrsId,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CoordinatePosition {
    Wgs84(Wgs84Position),
    Ecef(EcefPosition),
    Enu(EnuPosition),
    Ned(NedPosition),
    Projected(ProjectedPosition),
}

impl Wgs84Position {
    pub fn validate(&self) -> Result<(), CoordinateValueError> {
        if !self.latitude_deg.is_finite()
            || !self.longitude_deg.is_finite()
            || !self.height_m.is_finite()
        {
            return Err(CoordinateValueError::new("coordinates must be finite"));
        }
        if !(-90.0..=90.0).contains(&self.latitude_deg) {
            return Err(CoordinateValueError::new(
                "latitude_deg must be within [-90, 90]",
            ));
        }
        if !(-180.0..=180.0).contains(&self.longitude_deg) {
            return Err(CoordinateValueError::new(
                "longitude_deg must be within [-180, 180]",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateValueError {
    rule: &'static str,
}

impl CoordinateValueError {
    pub fn new(rule: &'static str) -> Self {
        Self { rule }
    }
}

impl fmt::Display for CoordinateValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.rule)
    }
}

impl std::error::Error for CoordinateValueError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Orientation3 {
    pub frame_id: FrameId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quaternion_xyzw: Option<[f64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw_pitch_roll_deg: Option<[f64; 3]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Pose3 {
    pub frame_id: FrameId,
    pub position: CoordinatePosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covariance: Option<Vec<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Velocity3 {
    pub frame_id: FrameId,
    pub x_mps: f64,
    pub y_mps: f64,
    pub z_mps: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrajectoryPoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<DateTime<Utc>>,
    pub pose: Pose3,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub velocity: Option<Velocity3>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryInterpolation {
    None,
    Linear,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Trajectory3 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trajectory_id: Option<TrajectoryId>,
    pub frame_id: FrameId,
    pub points: Vec<TrajectoryPoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpolation: Option<TrajectoryInterpolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FrameDefinition {
    pub frame_id: FrameId,
    pub kind: FrameKind,
    pub axis_convention: AxisConvention,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<FrameId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Wgs84Position>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crs: Option<CrsId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datum: Option<DatumId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ellipsoid: Option<EllipsoidId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epoch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GeofenceRule {
    MustStayInside,
    MustStayOutside,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LinearRing2 {
    pub coordinates: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Polygon2 {
    pub exterior: LinearRing2,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<LinearRing2>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Path2 {
    pub coordinates: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeofenceGeometry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geofence_id: Option<GeofenceId>,
    pub frame_id: FrameId,
    pub rule: GeofenceRule,
    pub polygon: Polygon2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateOperationKind {
    FrameConversion,
    CrsTransform,
    LocalFrameDerivation,
    GeodesicInverse,
    GeodesicDirect,
    GeofenceValidation,
    Batch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoordinateOperationRef {
    pub operation_id: CoordinateOperationId,
    pub operation_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_frame: Option<FrameId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_frame: Option<FrameId>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoordinateOperationProvenance {
    pub operation: CoordinateOperationRef,
    pub kind: CoordinateOperationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_crs: Option<CrsId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_crs: Option<CrsId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grid_packages: Vec<String>,
    #[serde(default)]
    pub approximation_used: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accuracy_m: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConvertFrameRequest {
    pub target_frame: FrameId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Wgs84Position>,
    pub points: Vec<CoordinatePosition>,
    #[serde(default)]
    pub allow_approximation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConvertFrameOutput {
    pub points: Vec<CoordinatePosition>,
    pub provenance: CoordinateOperationProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TransformCrsRequest {
    pub source_crs: CrsId,
    pub target_crs: CrsId,
    pub points: Vec<ProjectedPosition>,
    #[serde(default)]
    pub allow_approximation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TransformCrsOutput {
    pub points: Vec<ProjectedPosition>,
    pub provenance: CoordinateOperationProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeriveLocalFrameRequest {
    pub frame_id: FrameId,
    pub kind: FrameKind,
    pub origin: Wgs84Position,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeriveLocalFrameOutput {
    pub frame: FrameDefinition,
    pub provenance: CoordinateOperationProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeodesicInverseRequest {
    pub start: Wgs84Position,
    pub end: Wgs84Position,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeodesicInverseOutput {
    pub distance_m: f64,
    pub initial_azimuth_deg: f64,
    pub final_azimuth_deg: f64,
    pub provenance: CoordinateOperationProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeodesicDirectRequest {
    pub start: Wgs84Position,
    pub initial_azimuth_deg: f64,
    pub distance_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeodesicDirectOutput {
    pub end: Wgs84Position,
    pub final_azimuth_deg: f64,
    pub provenance: CoordinateOperationProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValidateGeofenceRequest {
    pub geofence: GeofenceGeometry,
    pub path: Path2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeofenceViolation {
    pub index: usize,
    pub point: [f64; 2],
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValidateGeofenceOutput {
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<GeofenceViolation>,
    pub provenance: CoordinateOperationProvenance,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BatchTransformRequest {
    pub convert: ConvertFrameRequest,
    #[serde(default)]
    pub artifact: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BatchTransformOutput {
    pub result: ConvertFrameOutput,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactMetadata>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_ids_accept_crs_and_frame_forms() {
        assert_eq!(CrsId::new("EPSG:4326").unwrap().as_str(), "EPSG:4326");
        assert_eq!(
            FrameId::new("ENU:mission-alpha").unwrap().as_str(),
            "ENU:mission-alpha"
        );
        assert!(FrameId::new("bad/id").is_err());
        assert!(FrameId::new("bad id").is_err());
    }

    #[test]
    fn wgs84_position_validation_rejects_invalid_ranges() {
        assert!(
            Wgs84Position {
                latitude_deg: 45.0,
                longitude_deg: -122.0,
                height_m: 10.0
            }
            .validate()
            .is_ok()
        );
        assert!(
            Wgs84Position {
                latitude_deg: 91.0,
                longitude_deg: -122.0,
                height_m: 10.0
            }
            .validate()
            .is_err()
        );
    }
}
