use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    DatasetReleaseId, Degrees, FacilityId, Kilograms, KilowattHours, Liters, LocationId, MapFamily,
    MapGeofenceId, Meters, MetersPerSecond, MobilityFamily, MobilityProfileId,
    OperationalSnapshotId, Ratio, RestrictionId, RouteId, RouteMatrixId, Seconds, ValidationId,
    Wgs84BoundingBox, Wgs84LineString, Wgs84Polygon, Wgs84Position,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FacilityKind {
    Depot,
    Warehouse,
    BorderCrossing,
    ChargingStation,
    FuelStation,
    RailTerminal,
    Port,
    Berth,
    Anchorage,
    Airport,
    Heliport,
    Vertiport,
    LandingZone,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceLineage {
    pub release_id: DatasetReleaseId,
    pub source_feature_id: String,
    pub authority: super::AuthorityClass,
    pub valid_from: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MapLocation {
    pub location_id: LocationId,
    pub name: String,
    pub position: Wgs84Position,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub alternate_names: BTreeSet<String>,
    pub lineage: SourceLineage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MapBoundary {
    pub boundary_id: super::MapBoundaryId,
    pub name: String,
    pub boundary_kind: String,
    pub geometry: Wgs84Polygon,
    pub lineage: SourceLineage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OperatingInterval {
    pub opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Facility {
    pub facility_id: FacilityId,
    pub name: String,
    pub kind: FacilityKind,
    pub position: Wgs84Position,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub supported_mobility_families: BTreeSet<MobilityFamily>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub transfer_map_families: BTreeSet<MapFamily>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operating_intervals: Vec<OperatingInterval>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub capabilities: BTreeSet<String>,
    pub lineage: SourceLineage,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RestrictionKind {
    Closure,
    Access,
    DimensionalLimit,
    WeightLimit,
    HazardousCargo,
    SpeedLimit,
    Environmental,
    ProtectedArea,
    NavigationalWarning,
    Airspace,
    Weather,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RestrictionEffectKind {
    Prohibit,
    Require,
    Limit,
    Penalize,
    Advise,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RestrictionLimit {
    MaximumHeight { value: Meters },
    MaximumWidth { value: Meters },
    MaximumLength { value: Meters },
    MaximumMass { value: Kilograms },
    MaximumSpeed { value: MetersPerSecond },
    MinimumDepth { value: Meters },
    MinimumAltitude { value: Meters },
    MaximumAltitude { value: Meters },
    MinimumReserve { value: Ratio },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RestrictionEffect {
    pub kind: RestrictionEffectKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<RestrictionLimit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerticalBand {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lower_m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upper_m: Option<f64>,
    pub reference: VerticalReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerticalReference {
    Ellipsoid,
    MeanSeaLevel,
    AboveGroundLevel,
    ChartDatum,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Restriction {
    pub restriction_id: RestrictionId,
    pub kind: RestrictionKind,
    pub geometry: Wgs84Polygon,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_band: Option<VerticalBand>,
    pub affected_mobility_families: BTreeSet<MobilityFamily>,
    pub effect: RestrictionEffect,
    pub valid_from: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
    pub authority: super::AuthorityClass,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_release_id: Option<DatasetReleaseId>,
    pub issued_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_by: Option<RestrictionId>,
    pub record_version: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RouteEndpoint {
    Position { position: Wgs84Position },
    Location { location_id: LocationId },
    Facility { facility_id: FacilityId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RouteObjectiveKind {
    Fastest,
    Shortest,
    LowestEnergy,
    LowestRisk,
    LowestCost,
    Weighted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ObjectiveWeights {
    pub duration: Ratio,
    pub distance: Ratio,
    pub energy: Ratio,
    pub risk: Ratio,
    pub cost: Ratio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteObjective {
    pub kind: RouteObjectiveKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weights: Option<ObjectiveWeights>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteConstraints {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_areas: Vec<Wgs84Polygon>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub avoided_areas: Vec<Wgs84Polygon>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_facility_stops: Vec<FacilityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_arrival: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_energy_reserve: Option<Ratio>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_authority_classes: BTreeSet<super::AuthorityClass>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteDataPolicy {
    #[serde(default)]
    pub allow_planning_advisory: bool,
    #[serde(default)]
    pub allow_stale_operational_data: bool,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_map_families: BTreeSet<MapFamily>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteRequest {
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: u64,
    pub origin: RouteEndpoint,
    pub destination: RouteEndpoint,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waypoints: Vec<RouteEndpoint>,
    pub departure_time: DateTime<Utc>,
    pub objective: RouteObjective,
    pub constraints: RouteConstraints,
    pub alternatives: u16,
    pub data_policy: RouteDataPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RouteStatus {
    PlanningAdvisory,
    Validated,
    Stale,
    Invalidated,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteCost {
    pub distance: Meters,
    pub duration: Seconds,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub energy: Option<KilowattHours>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel: Option<Liters>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monetary_minor_units: Option<u64>,
    pub risk: Ratio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteInstruction {
    pub sequence: u32,
    pub position: Wgs84Position,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<Degrees>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteLeg {
    pub sequence: u32,
    pub map_family: MapFamily,
    pub geometry: Wgs84LineString,
    pub cost: RouteCost,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<RouteInstruction>,
    pub source_release_ids: BTreeSet<DatasetReleaseId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub restriction_ids: BTreeSet<RestrictionId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteProvenance {
    pub base_release_ids: BTreeSet<DatasetReleaseId>,
    pub operational_snapshot_id: OperationalSnapshotId,
    pub planner_version: String,
    pub cost_model_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteAlternative {
    pub rank: u16,
    pub legs: Vec<RouteLeg>,
    pub summary: RouteCost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RoutePlan {
    pub route_id: RouteId,
    pub route_uri: String,
    pub status: RouteStatus,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: u64,
    pub departure_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrival_time: Option<DateTime<Utc>>,
    pub legs: Vec<RouteLeg>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<RouteAlternative>,
    pub summary: RouteCost,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub crossed_boundary_ids: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub facility_ids: BTreeSet<FacilityId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub restriction_ids: BTreeSet<RestrictionId>,
    pub validation_id: ValidationId,
    pub provenance: RouteProvenance,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteMatrixRequest {
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: u64,
    pub origins: Vec<RouteEndpoint>,
    pub destinations: Vec<RouteEndpoint>,
    pub departure_time: DateTime<Utc>,
    pub objective: RouteObjective,
    pub constraints: RouteConstraints,
    pub data_policy: RouteDataPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteMatrixCell {
    pub origin_index: u32,
    pub destination_index: u32,
    pub status: RouteStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<RouteCost>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteMatrix {
    pub matrix_id: RouteMatrixId,
    pub cells: Vec<RouteMatrixCell>,
    pub provenance: RouteProvenance,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReachableBudget {
    Duration { value: Seconds },
    Distance { value: Meters },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReachableAreaRequest {
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: u64,
    pub origin: RouteEndpoint,
    pub departure_time: DateTime<Utc>,
    pub budget: ReachableBudget,
    pub constraints: RouteConstraints,
    pub data_policy: RouteDataPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generalization: Option<Meters>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReachableArea {
    pub reachable_area_id: super::ReachableAreaId,
    pub reachable_area_uri: String,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: u64,
    pub origin: Wgs84Position,
    pub departure_time: DateTime<Utc>,
    pub budget: ReachableBudget,
    pub polygons: Vec<Wgs84Polygon>,
    pub provenance: RouteProvenance,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OperationalSnapshot {
    pub snapshot_id: OperationalSnapshotId,
    pub captured_at: DateTime<Utc>,
    pub departure_time: DateTime<Utc>,
    pub coverage: Wgs84BoundingBox,
    pub restriction_ids: BTreeSet<RestrictionId>,
    pub observation_release_ids: BTreeSet<DatasetReleaseId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Geofence {
    pub geofence_id: MapGeofenceId,
    pub area: Wgs84Polygon,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_band: Option<VerticalBand>,
}
