use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    ArtifactMetadata, CoordinateOperationProvenance, CrsId, FrameId, FrameKind, GeofenceViolation,
};
use veoveo_rrd::{RrdFrameDefinition, RrdGeoPoint, RrdGeofenceGeometry, RrdLocalLineString2};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Wgs84Position {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    #[serde(default)]
    pub height_m: f64,
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

    pub fn to_rrd_geo_point(&self) -> RrdGeoPoint {
        RrdGeoPoint {
            latitude_deg: self.latitude_deg,
            longitude_deg: self.longitude_deg,
            height_m: Some(self.height_m),
        }
    }
}

impl From<Wgs84Position> for RrdGeoPoint {
    fn from(value: Wgs84Position) -> Self {
        value.to_rrd_geo_point()
    }
}

impl TryFrom<RrdGeoPoint> for Wgs84Position {
    type Error = CoordinateValueError;

    fn try_from(value: RrdGeoPoint) -> Result<Self, Self::Error> {
        let position = Self {
            latitude_deg: value.latitude_deg,
            longitude_deg: value.longitude_deg,
            height_m: value.height_m.unwrap_or_default(),
        };
        position.validate()?;
        Ok(position)
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
pub enum CoordinatePoint {
    Wgs84(Wgs84Position),
    Ecef(EcefPosition),
    Enu(EnuPosition),
    Ned(NedPosition),
    Projected(ProjectedPosition),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConvertFrameRequest {
    pub target_frame: FrameId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Wgs84Position>,
    pub points: Vec<CoordinatePoint>,
    #[serde(default)]
    pub allow_approximation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConvertFrameOutput {
    pub points: Vec<CoordinatePoint>,
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
    pub frame: RrdFrameDefinition,
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
    pub geofence: RrdGeofenceGeometry,
    pub path: RrdLocalLineString2,
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
    fn wgs84_position_round_trips_through_rrd_geo_point() {
        let position = Wgs84Position {
            latitude_deg: 37.0,
            longitude_deg: -122.0,
            height_m: 10.0,
        };
        let rrd = position.to_rrd_geo_point();
        let back = Wgs84Position::try_from(rrd).unwrap();
        assert_eq!(position, back);
    }

    #[test]
    fn wgs84_position_validation_rejects_invalid_ranges() {
        assert!(
            Wgs84Position {
                latitude_deg: 45.0,
                longitude_deg: -122.0,
                height_m: 10.0,
            }
            .validate()
            .is_ok()
        );
        assert!(
            Wgs84Position {
                latitude_deg: 91.0,
                longitude_deg: -122.0,
                height_m: 10.0,
            }
            .validate()
            .is_err()
        );
    }
}
