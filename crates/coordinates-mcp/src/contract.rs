use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    ArtifactMetadata, CoordinateOperationProvenance, CoordinatePosition, CrsId, FrameDefinition,
    FrameId, FrameKind, GeofenceGeometry, Path2, ProjectedPosition, Wgs84Position,
};

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
pub struct ValidateGeofenceOutput {
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<veoveo_mcp_contract::GeofenceViolation>,
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
