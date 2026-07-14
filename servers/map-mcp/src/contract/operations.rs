use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::CrsId;

use super::{
    Degrees, Facility, LocationId, MapLocation, Meters, ProjectedPosition, Restriction,
    RestrictionId, RoutePlan, ValidationId, Wgs84BoundingBox, Wgs84LineString, Wgs84Polygon,
    Wgs84Position,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TransformCrsRequest {
    pub source_crs: CrsId,
    pub target_crs: CrsId,
    pub positions: Vec<ProjectedPosition>,
    #[serde(default)]
    pub allow_approximation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TransformCrsOutput {
    pub positions: Vec<ProjectedPosition>,
    pub engine: String,
    pub approximation_used: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<Meters>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeodesicInverseRequest {
    pub start: Wgs84Position,
    pub end: Wgs84Position,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeodesicInverseOutput {
    pub distance: Meters,
    pub initial_azimuth: Degrees,
    pub final_azimuth: Degrees,
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeodesicDirectRequest {
    pub start: Wgs84Position,
    pub initial_azimuth: Degrees,
    pub distance: Meters,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeodesicDirectOutput {
    pub end: Wgs84Position,
    pub final_azimuth: Degrees,
    pub engine: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GeofenceRule {
    MustRemainInside,
    MustRemainOutside,
    MustNotCrossBoundary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValidateGeofenceRequest {
    pub geofence: Wgs84Polygon,
    pub path: Wgs84LineString,
    pub rule: GeofenceRule,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GeofenceViolation {
    pub segment_index: u32,
    pub position: Wgs84Position,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValidateGeofenceOutput {
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<GeofenceViolation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SearchLocationsRequest {
    pub query: String,
    pub coverage: Wgs84BoundingBox,
    #[serde(default)]
    pub include_facilities: bool,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SearchLocationsOutput {
    pub locations: Vec<MapLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facilities: Vec<Facility>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InspectLocationRequest {
    pub location_id: LocationId,
    pub nearby_radius: Meters,
    pub facility_limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InspectLocationOutput {
    pub location: MapLocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nearby_facilities: Vec<Facility>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub containing_boundary_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_gaps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CorridorInspectionRequest {
    pub corridor: Wgs84LineString,
    pub width: Meters,
    pub departure_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CorridorInspectionOutput {
    pub restrictions: Vec<Restriction>,
    pub facilities: Vec<Facility>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub crossed_boundary_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_gaps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ValidateRouteRequest {
    pub route: RoutePlan,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteValidation {
    pub validation_id: ValidationId,
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<String>,
    pub validated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PublishRestrictionRequest {
    pub restriction: Restriction,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WithdrawRestrictionRequest {
    pub restriction_id: RestrictionId,
    pub expected_record_version: u64,
    pub effective_at: chrono::DateTime<chrono::Utc>,
    pub cancellation_restriction_id: RestrictionId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RestrictionMutationOutput {
    pub restriction: Restriction,
    pub invalidated_route_count: u64,
}
