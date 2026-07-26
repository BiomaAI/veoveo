use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{PrincipalId, WorkContextId};

use super::{
    DatasetReleaseId, Degrees, FeatureGeometry, Meters, MobilityProfileId, RestrictionId,
    SpatialDerivationId, Wgs84LineString, Wgs84Polygon, Wgs84Position,
};

pub const SPATIAL_DERIVATION_SCHEMA_VERSION: u64 = 1;
pub const SPATIAL_DERIVATION_ALGORITHM_REVISION: &str =
    "map-spatial-local-equirectangular-wgs84-v1";
pub const MAX_SPATIAL_INPUT_COORDINATES: usize = 10_000;
pub const MAX_SPATIAL_OUTPUT_COORDINATES: usize = 50_000;
pub const MAX_SPATIAL_POINT_INPUTS: usize = 512;
pub const MAX_SPATIAL_COMPONENT_INPUTS: usize = 512;
pub const MAX_PARALLEL_LANES: u32 = 128;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpatialPointInput {
    pub id: String,
    pub position: Wgs84Position,
}

impl SpatialPointInput {
    fn validate(&self) -> Result<(), SpatialContractError> {
        validate_controlled(&self.id, 256)?;
        self.position
            .validate()
            .map_err(|_| SpatialContractError::InvalidGeometry)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpatialGeometryInput {
    pub id: String,
    pub geometry: FeatureGeometry,
}

impl SpatialGeometryInput {
    fn validate(&self) -> Result<(), SpatialContractError> {
        validate_controlled(&self.id, 256)?;
        self.geometry
            .validate()
            .map_err(|_| SpatialContractError::InvalidGeometry)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StandoffSide {
    Inward,
    Outward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnDirection {
    Clockwise,
    CounterClockwise,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StationKind {
    Relay,
    Station,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SpatialDerivationOperation {
    ResampleLine {
        line: Wgs84LineString,
        maximum_segment_length: Meters,
    },
    OrderPoints {
        points: Vec<SpatialPointInput>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_id: Option<String>,
        #[serde(default)]
        close_tour: bool,
    },
    PolygonBoundary {
        polygon: Wgs84Polygon,
    },
    StandoffPerimeter {
        polygon: Wgs84Polygon,
        distance: Meters,
        side: StandoffSide,
    },
    Corridor {
        centerline: Wgs84LineString,
        half_width: Meters,
    },
    ParallelLanes {
        centerline: Wgs84LineString,
        lane_count: u32,
        lane_spacing: Meters,
        corridor_half_width: Meters,
    },
    Racetrack {
        center: Wgs84Position,
        heading: Degrees,
        straight_length: Meters,
        turn_radius: Meters,
        direction: TurnDirection,
        sample_spacing: Meters,
    },
    Stations {
        line: Wgs84LineString,
        spacing: Meters,
        station_kind: StationKind,
    },
    Coverage {
        area: Wgs84Polygon,
        lane_spacing: Meters,
        heading: Degrees,
        boundary_standoff: Meters,
    },
    ConnectedComponents {
        geometries: Vec<SpatialGeometryInput>,
        connection_tolerance: Meters,
    },
    Ingress {
        target: Wgs84Position,
        inbound_heading: Degrees,
        lead_in_distance: Meters,
        final_approach_distance: Meters,
    },
    ValidateRoute {
        route: Wgs84LineString,
    },
}

impl SpatialDerivationOperation {
    pub fn validate(&self) -> Result<(), SpatialContractError> {
        match self {
            Self::ResampleLine {
                line,
                maximum_segment_length,
            } => {
                validate_line(line)?;
                positive(*maximum_segment_length)
            }
            Self::OrderPoints {
                points, start_id, ..
            } => {
                if !(2..=MAX_SPATIAL_POINT_INPUTS).contains(&points.len()) {
                    return Err(SpatialContractError::InvalidCardinality);
                }
                points.iter().try_for_each(SpatialPointInput::validate)?;
                let ids = points
                    .iter()
                    .map(|point| point.id.as_str())
                    .collect::<BTreeSet<_>>();
                if ids.len() != points.len()
                    || start_id
                        .as_ref()
                        .is_some_and(|id| !ids.contains(id.as_str()))
                {
                    return Err(SpatialContractError::InvalidPointIdentity);
                }
                Ok(())
            }
            Self::PolygonBoundary { polygon } => validate_polygon(polygon),
            Self::StandoffPerimeter {
                polygon, distance, ..
            } => {
                validate_polygon(polygon)?;
                positive(*distance)
            }
            Self::Corridor {
                centerline,
                half_width,
            } => {
                validate_line(centerline)?;
                positive(*half_width)
            }
            Self::ParallelLanes {
                centerline,
                lane_count,
                lane_spacing,
                corridor_half_width,
            } => {
                validate_line(centerline)?;
                if !(1..=MAX_PARALLEL_LANES).contains(lane_count) {
                    return Err(SpatialContractError::InvalidCardinality);
                }
                positive(*lane_spacing)?;
                positive(*corridor_half_width)?;
                let required_half_width =
                    f64::from(lane_count.saturating_sub(1)) * lane_spacing.get() / 2.0;
                if required_half_width > corridor_half_width.get() {
                    return Err(SpatialContractError::InvalidGeometry);
                }
                Ok(())
            }
            Self::Racetrack {
                center,
                heading,
                straight_length,
                turn_radius,
                sample_spacing,
                ..
            } => {
                validate_position(center)?;
                validate_heading(*heading)?;
                positive(*straight_length)?;
                positive(*turn_radius)?;
                positive(*sample_spacing)
            }
            Self::Stations { line, spacing, .. } => {
                validate_line(line)?;
                positive(*spacing)
            }
            Self::Coverage {
                area,
                lane_spacing,
                heading,
                ..
            } => {
                validate_polygon(area)?;
                positive(*lane_spacing)?;
                validate_heading(*heading)
            }
            Self::ConnectedComponents {
                geometries,
                connection_tolerance: _,
            } => {
                if geometries.is_empty() || geometries.len() > MAX_SPATIAL_COMPONENT_INPUTS {
                    return Err(SpatialContractError::InvalidCardinality);
                }
                geometries
                    .iter()
                    .try_for_each(SpatialGeometryInput::validate)?;
                let ids = geometries
                    .iter()
                    .map(|geometry| geometry.id.as_str())
                    .collect::<BTreeSet<_>>();
                if ids.len() != geometries.len() {
                    return Err(SpatialContractError::InvalidPointIdentity);
                }
                Ok(())
            }
            Self::Ingress {
                target,
                inbound_heading,
                lead_in_distance,
                final_approach_distance,
            } => {
                validate_position(target)?;
                validate_heading(*inbound_heading)?;
                positive(*lead_in_distance)?;
                positive(*final_approach_distance)?;
                if final_approach_distance >= lead_in_distance {
                    return Err(SpatialContractError::InvalidGeometry);
                }
                Ok(())
            }
            Self::ValidateRoute { route } => validate_line(route),
        }
    }

    pub fn input_coordinate_count(&self) -> usize {
        match self {
            Self::ResampleLine { line, .. }
            | Self::Corridor {
                centerline: line, ..
            }
            | Self::ParallelLanes {
                centerline: line, ..
            }
            | Self::Stations { line, .. }
            | Self::ValidateRoute { route: line } => line.coordinates.len(),
            Self::OrderPoints { points, .. } => points.len(),
            Self::PolygonBoundary { polygon }
            | Self::StandoffPerimeter { polygon, .. }
            | Self::Coverage { area: polygon, .. } => {
                polygon.exterior.len() + polygon.interiors.iter().map(Vec::len).sum::<usize>()
            }
            Self::Racetrack { .. } | Self::Ingress { .. } => 1,
            Self::ConnectedComponents { geometries, .. } => geometries
                .iter()
                .map(|geometry| geometry.geometry.coordinate_count())
                .sum(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeriveSpatialGeometryRequest {
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: u64,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub source_release_ids: BTreeSet<DatasetReleaseId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub terrain_classes: BTreeSet<String>,
    pub effective_at: DateTime<Utc>,
    pub operation: SpatialDerivationOperation,
    pub algorithm_revision: String,
}

impl DeriveSpatialGeometryRequest {
    pub fn validate(&self) -> Result<(), SpatialContractError> {
        if self.mobility_profile_version == 0
            || self.algorithm_revision != SPATIAL_DERIVATION_ALGORITHM_REVISION
            || self.operation.input_coordinate_count() > MAX_SPATIAL_INPUT_COORDINATES
        {
            return Err(SpatialContractError::UnsupportedRequest);
        }
        validate_text_set(&self.terrain_classes)?;
        self.operation.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpatialGeometryRole {
    ResampledLine,
    OrderedTour,
    Boundary,
    StandoffPerimeter,
    Corridor,
    ParallelLane,
    Racetrack,
    RelayStations,
    Stations,
    CoverageTrack,
    Component,
    Ingress,
    ValidatedRoute,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpatialGeometry {
    pub role: SpatialGeometryRole,
    pub ordinal: u32,
    pub geometry: FeatureGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpatialFindingSeverity {
    Advisory,
    Violation,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SpatialFindingCode {
    TerrainClassNotAllowed,
    RestrictionClassNotAllowed,
    ProhibitedRestriction,
    RequiredRestrictionCondition,
    RestrictionLimitExceeded,
    RestrictionAdvisory,
    RoutePointLimitExceeded,
    SegmentLengthExceeded,
    RangeExceeded,
    TurnRadiusExceeded,
    ClimbLimitExceeded,
    DescentLimitExceeded,
    CeilingExceeded,
    VerticalReferenceUnavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpatialFinding {
    pub severity: SpatialFindingSeverity,
    pub code: SpatialFindingCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restriction_id: Option<RestrictionId>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpatialProjection {
    pub profile: String,
    pub origin: Wgs84Position,
    pub earth_radius_m: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpatialDerivation {
    pub schema_version: u64,
    pub derivation_id: SpatialDerivationId,
    pub resource_uri: String,
    pub operation: SpatialDerivationOperation,
    pub geometries: Vec<SpatialGeometry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ordered_input_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub connected_components: Vec<Vec<String>>,
    pub valid: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<SpatialFinding>,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: u64,
    pub source_release_ids: BTreeSet<DatasetReleaseId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub intersected_restriction_ids: BTreeSet<RestrictionId>,
    pub terrain_classes: BTreeSet<String>,
    pub effective_at: DateTime<Utc>,
    pub projection: SpatialProjection,
    pub algorithm_revision: String,
    pub request_digest_sha256: String,
    pub geometry_digest_sha256: String,
    pub created_by: PrincipalId,
    pub work_context: WorkContextId,
    pub created_at: DateTime<Utc>,
}

impl SpatialDerivation {
    pub fn validate(&self) -> Result<(), SpatialContractError> {
        if self.schema_version != SPATIAL_DERIVATION_SCHEMA_VERSION
            || self.resource_uri
                != format!("map://spatial-derivation/{}", self.derivation_id.as_str())
            || self.algorithm_revision != SPATIAL_DERIVATION_ALGORITHM_REVISION
            || self.geometries.is_empty()
            || self
                .geometries
                .iter()
                .map(|geometry| geometry.geometry.coordinate_count())
                .sum::<usize>()
                > MAX_SPATIAL_OUTPUT_COORDINATES
            || self.valid
                != self
                    .findings
                    .iter()
                    .all(|finding| finding.severity != SpatialFindingSeverity::Violation)
        {
            return Err(SpatialContractError::InvalidDerivation);
        }
        validate_sha256(&self.request_digest_sha256)?;
        validate_sha256(&self.geometry_digest_sha256)?;
        validate_controlled(&self.projection.profile, 128)?;
        if !self.projection.earth_radius_m.is_finite() || self.projection.earth_radius_m <= 0.0 {
            return Err(SpatialContractError::InvalidDerivation);
        }
        self.projection
            .origin
            .validate()
            .map_err(|_| SpatialContractError::InvalidDerivation)?;
        self.geometries.iter().try_for_each(|geometry| {
            geometry
                .geometry
                .validate()
                .map_err(|_| SpatialContractError::InvalidDerivation)
        })?;
        DeriveSpatialGeometryRequest {
            mobility_profile_id: self.mobility_profile_id.clone(),
            mobility_profile_version: self.mobility_profile_version,
            source_release_ids: self.source_release_ids.clone(),
            terrain_classes: self.terrain_classes.clone(),
            effective_at: self.effective_at,
            operation: self.operation.clone(),
            algorithm_revision: self.algorithm_revision.clone(),
        }
        .validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpatialContractError {
    InvalidControlledValue,
    InvalidGeometry,
    InvalidCardinality,
    InvalidPointIdentity,
    UnsupportedRequest,
    InvalidDerivation,
    InvalidDigest,
}

impl std::fmt::Display for SpatialContractError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidControlledValue => "spatial controlled value is invalid",
            Self::InvalidGeometry => "spatial geometry or distance is invalid",
            Self::InvalidCardinality => "spatial input cardinality exceeds its bound",
            Self::InvalidPointIdentity => "spatial point or geometry identities must be unique",
            Self::UnsupportedRequest => "spatial request schema or algorithm is unsupported",
            Self::InvalidDerivation => "spatial derivation is internally inconsistent",
            Self::InvalidDigest => "spatial digest must be 64 lowercase hexadecimal characters",
        })
    }
}

impl std::error::Error for SpatialContractError {}

fn positive(value: Meters) -> Result<(), SpatialContractError> {
    if value.get() <= 0.0 {
        return Err(SpatialContractError::InvalidGeometry);
    }
    Ok(())
}

fn validate_line(line: &Wgs84LineString) -> Result<(), SpatialContractError> {
    line.validate()
        .map_err(|_| SpatialContractError::InvalidGeometry)
}

fn validate_polygon(polygon: &Wgs84Polygon) -> Result<(), SpatialContractError> {
    polygon
        .validate()
        .map_err(|_| SpatialContractError::InvalidGeometry)
}

fn validate_position(position: &Wgs84Position) -> Result<(), SpatialContractError> {
    position
        .validate()
        .map_err(|_| SpatialContractError::InvalidGeometry)
}

fn validate_heading(heading: Degrees) -> Result<(), SpatialContractError> {
    if heading.get() > 360.0 {
        return Err(SpatialContractError::InvalidGeometry);
    }
    Ok(())
}

fn validate_controlled(value: &str, maximum_bytes: usize) -> Result<(), SpatialContractError> {
    if value.is_empty()
        || value.len() > maximum_bytes
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(SpatialContractError::InvalidControlledValue);
    }
    Ok(())
}

fn validate_text_set(values: &BTreeSet<String>) -> Result<(), SpatialContractError> {
    values
        .iter()
        .try_for_each(|value| validate_controlled(value, 128))
}

fn validate_sha256(value: &str) -> Result<(), SpatialContractError> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(SpatialContractError::InvalidDigest);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(longitude_deg: f64, latitude_deg: f64) -> Wgs84Position {
        Wgs84Position::new(longitude_deg, latitude_deg, None).unwrap()
    }

    #[test]
    fn parallel_lanes_must_fit_the_declared_corridor() {
        let operation = SpatialDerivationOperation::ParallelLanes {
            centerline: Wgs84LineString {
                coordinates: vec![point(-89.2, 13.6), point(-89.1, 13.7)],
            },
            lane_count: 4,
            lane_spacing: Meters::new(10.0).unwrap(),
            corridor_half_width: Meters::new(14.0).unwrap(),
        };
        assert_eq!(
            operation.validate(),
            Err(SpatialContractError::InvalidGeometry)
        );
    }

    #[test]
    fn point_ordering_rejects_duplicate_stable_identities() {
        let operation = SpatialDerivationOperation::OrderPoints {
            points: vec![
                SpatialPointInput {
                    id: "point-a".to_owned(),
                    position: point(-89.2, 13.6),
                },
                SpatialPointInput {
                    id: "point-a".to_owned(),
                    position: point(-89.1, 13.7),
                },
            ],
            start_id: None,
            close_tour: false,
        };
        assert_eq!(
            operation.validate(),
            Err(SpatialContractError::InvalidPointIdentity)
        );
    }
}
