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
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SourceLineage {
    pub release_id: DatasetReleaseId,
    pub source_feature_id: super::SourceFeatureId,
    pub authority: super::AuthorityClass,
    pub valid_from: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct MapLocation {
    pub location_id: LocationId,
    pub name: String,
    pub position: Wgs84Position,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub alternate_names: BTreeSet<String>,
    pub lineage: SourceLineage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct MapBoundary {
    pub boundary_id: super::MapBoundaryId,
    pub name: String,
    pub boundary_kind: String,
    pub geometry: Wgs84Polygon,
    pub lineage: SourceLineage,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct OperatingInterval {
    pub opens_at: DateTime<Utc>,
    pub closes_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
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
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct RestrictionEffect {
    pub kind: RestrictionEffectKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<RestrictionLimit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
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
#[serde(deny_unknown_fields)]
#[schemars(rename = "Restriction")]
#[serde(rename_all = "camelCase")]
pub struct RestrictionValue {
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
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveWeights {
    pub duration: Ratio,
    pub distance: Ratio,
    pub energy: Ratio,
    pub risk: Ratio,
    pub cost: Ratio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct RouteObjective {
    pub kind: RouteObjectiveKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weights: Option<ObjectiveWeights>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
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
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct RouteDataPolicy {
    #[serde(default)]
    pub allow_planning_advisory: bool,
    #[serde(default)]
    pub allow_stale_operational_data: bool,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_map_families: BTreeSet<MapFamily>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct RouteRequest {
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: crate::contract::MobilityProfileVersion,
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
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
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
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct RouteInstruction {
    pub sequence: u32,
    pub position: Wgs84Position,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<Degrees>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
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
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct RouteProvenance {
    pub base_release_ids: BTreeSet<DatasetReleaseId>,
    pub operational_snapshot_id: OperationalSnapshotId,
    pub planner_version: String,
    pub cost_model_version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct RouteAlternative {
    pub rank: u16,
    pub legs: Vec<RouteLeg>,
    pub summary: RouteCost,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "RoutePlan")]
#[serde(rename_all = "camelCase")]
pub struct RoutePlanValue {
    pub route_id: RouteId,
    pub route_uri: super::MapRouteUri,
    pub status: RouteStatus,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: crate::contract::MobilityProfileVersion,
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
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct RouteMatrixRequest {
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: crate::contract::MobilityProfileVersion,
    pub origins: Vec<RouteEndpoint>,
    pub destinations: Vec<RouteEndpoint>,
    pub departure_time: DateTime<Utc>,
    pub objective: RouteObjective,
    pub constraints: RouteConstraints,
    pub data_policy: RouteDataPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct RouteMatrixCell {
    pub origin_index: u32,
    pub destination_index: u32,
    pub status: RouteStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<RouteCost>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct RouteMatrix {
    pub matrix_id: RouteMatrixId,
    pub cells: Vec<RouteMatrixCell>,
    pub provenance: RouteProvenance,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[serde(deny_unknown_fields)]
pub enum ReachableBudget {
    Duration { value: Seconds },
    Distance { value: Meters },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ReachableAreaRequest {
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: crate::contract::MobilityProfileVersion,
    pub origin: RouteEndpoint,
    pub departure_time: DateTime<Utc>,
    pub budget: ReachableBudget,
    pub constraints: RouteConstraints,
    pub data_policy: RouteDataPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generalization: Option<Meters>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ReachableArea {
    pub reachable_area_id: super::ReachableAreaId,
    pub mobility_profile_id: MobilityProfileId,
    pub mobility_profile_version: crate::contract::MobilityProfileVersion,
    pub origin: Wgs84Position,
    pub departure_time: DateTime<Utc>,
    pub budget: ReachableBudget,
    pub polygons: Vec<Wgs84Polygon>,
    pub provenance: RouteProvenance,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct OperationalSnapshot {
    pub snapshot_id: OperationalSnapshotId,
    pub captured_at: DateTime<Utc>,
    pub departure_time: DateTime<Utc>,
    pub coverage: Wgs84BoundingBox,
    pub restriction_ids: BTreeSet<RestrictionId>,
    pub observation_release_ids: BTreeSet<DatasetReleaseId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Geofence {
    pub geofence_id: MapGeofenceId,
    pub area: Wgs84Polygon,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_band: Option<VerticalBand>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RoutePlan(veoveo_types::Checked<RoutePlanValue>);
impl RoutePlan {
    pub fn new(value: RoutePlanValue) -> Result<Self, super::MapRelationshipError> {
        veoveo_types::Checked::new(value).map(Self)
    }
    pub fn into_value(self) -> RoutePlanValue {
        self.0.into_inner()
    }
}
impl std::ops::Deref for RoutePlan {
    type Target = RoutePlanValue;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}
impl veoveo_types::Check for RoutePlanValue {
    type Error = super::MapRelationshipError;
    fn check(&self) -> Result<(), Self::Error> {
        super::relationships::check_route(self)
    }
}
impl JsonSchema for RoutePlan {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        RoutePlanValue::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        RoutePlanValue::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        RoutePlanValue::json_schema(generator)
    }
}

/// Mutable lifecycle state is rechecked on decoding, construction and every serialization.
#[derive(Debug, Clone, PartialEq)]
pub struct Restriction(RestrictionValue);
impl Restriction {
    pub fn new(value: RestrictionValue) -> Result<Self, super::MapRelationshipError> {
        veoveo_types::Check::check(&value)?;
        Ok(Self(value))
    }
    pub fn into_value(self) -> RestrictionValue {
        self.0
    }
}
impl std::ops::Deref for Restriction {
    type Target = RestrictionValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Restriction {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl veoveo_types::Check for RestrictionValue {
    type Error = super::MapRelationshipError;
    fn check(&self) -> Result<(), Self::Error> {
        super::relationships::check_restriction(self)
    }
}
impl Serialize for Restriction {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        veoveo_types::Check::check(&self.0).map_err(serde::ser::Error::custom)?;
        self.0.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Restriction {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(RestrictionValue::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
impl JsonSchema for Restriction {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        RestrictionValue::schema_name()
    }
    fn schema_id() -> std::borrow::Cow<'static, str> {
        RestrictionValue::schema_id()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        RestrictionValue::json_schema(generator)
    }
}
