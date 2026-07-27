use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    ArtifactMetadata, FrameWorldRevisionUri, PolicyVersion, PrincipalId, WorkContextId,
    parse_artifact_plane_uri,
};

pub const PLAN_SCHEMA_VERSION: u64 = 1;
pub const PLAN_ALGORITHM_REVISION: &str = "optimization-spatial-assignment-microlp-v1";
pub const MAX_AGENTS: usize = 512;
pub const MAX_GROUPS: usize = 128;
pub const MAX_TASKS: usize = 512;
pub const MAX_SHARED_RESOURCES: usize = 256;
pub const MAX_LANES: usize = 256;
pub const MAX_RESOURCE_BANDS: usize = 256;
pub const MAX_MUTUAL_EXCLUSION_SETS: usize = 256;
pub const MAX_GENERATED_CANDIDATES: u32 = 50_000;

macro_rules! controlled_id {
    ($name:ident, $label:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        #[schemars(with = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, PlanContractError> {
                let value = value.into();
                if value.is_empty()
                    || value.len() > 128
                    || value.trim() != value
                    || value.chars().any(|character| {
                        !(character.is_ascii_alphanumeric()
                            || matches!(character, '-' | '_' | '.' | ':'))
                    })
                {
                    return Err(PlanContractError::InvalidIdentifier($label));
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = PlanContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

controlled_id!(AgentId, "agent id");
controlled_id!(AgentGroupId, "agent group id");
controlled_id!(SpatialTaskId, "spatial task id");
controlled_id!(CapabilityId, "capability id");
controlled_id!(SharedResourceId, "shared resource id");
controlled_id!(LaneId, "lane id");
controlled_id!(ResourceBandId, "resource band id");
controlled_id!(MutualExclusionId, "mutual-exclusion id");

macro_rules! stable_uuid_id {
    ($name:ident, $prefix:literal, $namespace:expr) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        #[schemars(with = "String")]
        pub struct $name(String);

        impl $name {
            pub fn from_stable_key(key: &[u8]) -> Self {
                let namespace = uuid::Uuid::from_u128($namespace);
                Self(format!(
                    "{}{}",
                    $prefix,
                    uuid::Uuid::new_v5(&namespace, key)
                ))
            }

            pub fn parse(value: impl Into<String>) -> Result<Self, PlanContractError> {
                let value = value.into();
                let uuid = value
                    .strip_prefix($prefix)
                    .and_then(|suffix| uuid::Uuid::parse_str(suffix).ok())
                    .filter(|uuid| uuid.get_version_num() == 5)
                    .ok_or(PlanContractError::InvalidIdentifier($prefix))?;
                Ok(Self(format!("{}{}", $prefix, uuid)))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = PlanContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

stable_uuid_id!(PlanId, "plan-", 0x90d8_f5ac_0108_50ea_a0e1_ed2c_a138_e86a);
stable_uuid_id!(
    PlanAssignmentId,
    "assignment-",
    0x3d1f_2058_ba9f_5cd9_9a46_1f24_b30d_34f7
);

macro_rules! uri_type {
    ($name:ident, $validator:ident, $label:literal) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        #[schemars(with = "String")]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, PlanContractError> {
                let value = value.into();
                $validator(&value)
                    .then_some(Self(value))
                    .ok_or(PlanContractError::InvalidUri($label))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = PlanContractError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

uri_type!(MapReleaseUri, valid_map_release_uri, "Map release URI");
uri_type!(
    MapMobilityProfileUri,
    valid_map_mobility_profile_uri,
    "Map mobility-profile URI"
);
uri_type!(
    MapSourceFeatureUri,
    valid_map_source_feature_uri,
    "Map source-feature URI"
);
uri_type!(
    MapSpatialDerivationUri,
    valid_map_spatial_derivation_uri,
    "Map spatial-derivation URI"
);
uri_type!(MapRouteUri, valid_map_route_uri, "Map route URI");
uri_type!(
    ArtifactTrajectoryUri,
    valid_artifact_uri,
    "artifact trajectory URI"
);

impl MapReleaseUri {
    pub fn release_id(&self) -> &str {
        self.0
            .split_once("/release/")
            .expect("validated Map release URI")
            .1
    }
}

impl MapSourceFeatureUri {
    pub fn release_id(&self) -> &str {
        self.0
            .strip_prefix("map://source-feature/")
            .and_then(|suffix| suffix.split_once('/'))
            .expect("validated Map source-feature URI")
            .0
    }
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.contains(['/', '?', '#'])
        && !value.chars().any(char::is_control)
}

fn valid_map_release_uri(value: &str) -> bool {
    value
        .strip_prefix("map://dataset/")
        .and_then(|suffix| suffix.split_once("/release/"))
        .is_some_and(|(dataset, release)| valid_segment(dataset) && valid_segment(release))
}

fn valid_map_mobility_profile_uri(value: &str) -> bool {
    value
        .strip_prefix("map://mobility-profile/")
        .and_then(|suffix| suffix.split_once('/'))
        .is_some_and(|(profile, version)| {
            valid_segment(profile) && version.parse::<u64>().is_ok_and(|version| version > 0)
        })
}

fn valid_map_source_feature_uri(value: &str) -> bool {
    value
        .strip_prefix("map://source-feature/")
        .and_then(|suffix| suffix.split_once('/'))
        .is_some_and(|(release, feature)| valid_segment(release) && valid_segment(feature))
}

fn valid_map_spatial_derivation_uri(value: &str) -> bool {
    value
        .strip_prefix("map://spatial-derivation/")
        .is_some_and(valid_segment)
}

fn valid_map_route_uri(value: &str) -> bool {
    value
        .strip_prefix("map://route/")
        .is_some_and(valid_segment)
}

fn valid_artifact_uri(value: &str) -> bool {
    parse_artifact_plane_uri(value).is_some()
}

macro_rules! finite_number {
    ($name:ident, $predicate:expr, $label:literal, $default:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
        #[serde(try_from = "f64", into = "f64")]
        #[schemars(with = "f64")]
        pub struct $name(f64);

        impl $name {
            pub fn new(value: f64) -> Result<Self, PlanContractError> {
                if !value.is_finite() || !($predicate)(value) {
                    return Err(PlanContractError::InvalidNumber($label));
                }
                Ok(Self(value))
            }

            pub const fn get(self) -> f64 {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self($default)
            }
        }

        impl TryFrom<f64> for $name {
            type Error = PlanContractError;

            fn try_from(value: f64) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for f64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

finite_number!(
    NonNegative,
    |value: f64| value >= 0.0,
    "non-negative value",
    0.0
);
finite_number!(Positive, |value: f64| value > 0.0, "positive value", 1.0);
finite_number!(
    Confidence,
    |value: f64| (0.0..=1.0).contains(&value),
    "confidence",
    1.0
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanRequest {
    pub schema_version: u64,
    pub source_map_releases: BTreeSet<MapReleaseUri>,
    pub frame_world_revision: FrameWorldRevisionUri,
    pub agents: Vec<PlanningAgent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<PlanningAgentGroup>,
    pub tasks: Vec<SpatialPlanningTask>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shared_resources: Vec<SharedResource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lanes: Vec<PlanningLane>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource_bands: Vec<PlanningResourceBand>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mutual_exclusions: Vec<TaskMutualExclusion>,
    #[serde(default)]
    pub objective: PlanningObjective,
    #[serde(default)]
    pub solver: PlanningSolverPolicy,
    #[serde(default)]
    pub artifacts: PlanArtifactOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningAgent {
    pub agent_id: AgentId,
    pub mobility_profile: MapMobilityProfileUri,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub capabilities: BTreeSet<CapabilityId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_capacities: BTreeMap<SharedResourceId, Positive>,
    #[serde(default = "default_max_assignments")]
    pub maximum_assignments: u32,
    #[serde(default)]
    pub assignment_cost: NonNegative,
    #[serde(default)]
    pub assignment_risk: NonNegative,
    #[serde(default)]
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningAgentGroup {
    pub group_id: AgentGroupId,
    pub member_agent_ids: BTreeSet<AgentId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub capabilities: BTreeSet<CapabilityId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpatialPlanningTask {
    pub task_id: SpatialTaskId,
    pub quantity: TaskQuantity,
    #[serde(default)]
    pub admission: TaskAdmission,
    #[serde(default)]
    pub assignment_unit: AssignmentUnitKind,
    #[serde(default)]
    pub priority: Positive,
    pub target: MapGeometryReference,
    pub execution: PlanExecutionReference,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_capabilities: BTreeSet<CapabilityId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_mobility_profiles: BTreeSet<MapMobilityProfileUri>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub eligible_agent_ids: BTreeSet<AgentId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub eligible_group_ids: BTreeSet<AgentGroupId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub depends_on: BTreeSet<SpatialTaskId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shared_resource_demand: BTreeMap<SharedResourceId, NonNegative>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agent_resource_demand: BTreeMap<SharedResourceId, NonNegative>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_lane_ids: BTreeSet<LaneId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_resource_band_ids: BTreeSet<ResourceBandId>,
    #[serde(default)]
    pub timing: TaskTiming,
    #[serde(default)]
    pub recurrence: PlanRecurrence,
    #[serde(default)]
    pub assignment_cost: NonNegative,
    #[serde(default)]
    pub assignment_risk: NonNegative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskQuantity {
    pub minimum: u32,
    pub desired: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskAdmission {
    #[default]
    Required,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentUnitKind {
    #[default]
    Agent,
    Group,
    AgentOrGroup,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MapGeometryReference {
    SourceFeature { uri: MapSourceFeatureUri },
    SpatialDerivation { uri: MapSpatialDerivationUri },
}

impl MapGeometryReference {
    pub fn uri(&self) -> &str {
        match self {
            Self::SourceFeature { uri } => uri.as_str(),
            Self::SpatialDerivation { uri } => uri.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanExecutionReference {
    MapRoute { uri: MapRouteUri },
    MapSpatialDerivation { uri: MapSpatialDerivationUri },
    ArtifactTrajectory { uri: ArtifactTrajectoryUri },
}

impl PlanExecutionReference {
    pub fn uri(&self) -> &str {
        match self {
            Self::MapRoute { uri } => uri.as_str(),
            Self::MapSpatialDerivation { uri } => uri.as_str(),
            Self::ArtifactTrajectory { uri } => uri.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SharedResource {
    pub resource_id: SharedResourceId,
    pub capacity: Positive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningLane {
    pub lane_id: LaneId,
    pub capacity: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry: Option<MapSpatialDerivationUri>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningResourceBand {
    pub resource_band_id: ResourceBandId,
    pub capacity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TaskMutualExclusion {
    pub exclusion_id: MutualExclusionId,
    pub task_ids: BTreeSet<SpatialTaskId>,
    pub maximum_active_tasks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskTiming {
    #[default]
    Unscheduled,
    FixedWindow {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
}

impl TaskTiming {
    pub fn overlaps(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::FixedWindow {
                    start: left_start,
                    end: left_end,
                },
                Self::FixedWindow {
                    start: right_start,
                    end: right_end,
                },
            ) => left_start < right_end && right_start < left_end,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanRecurrence {
    #[default]
    Once,
    Loop {
        repetitions: u32,
    },
    Periodic {
        occurrences: u32,
        interval_seconds: Positive,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningObjective {
    pub priority_weight: NonNegative,
    pub cost_weight: NonNegative,
    pub risk_weight: NonNegative,
    pub confidence_weight: NonNegative,
    pub resource_weight: NonNegative,
}

impl Default for PlanningObjective {
    fn default() -> Self {
        Self {
            priority_weight: NonNegative::new(1.0).expect("valid default"),
            cost_weight: NonNegative::new(1.0).expect("valid default"),
            risk_weight: NonNegative::new(1.0).expect("valid default"),
            confidence_weight: NonNegative::new(0.25).expect("valid default"),
            resource_weight: NonNegative::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlanningSolverBackend {
    #[default]
    MicroLp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningSolverPolicy {
    #[serde(default)]
    pub backend: PlanningSolverBackend,
    #[serde(default)]
    pub deterministic_seed: u64,
    #[serde(default = "default_maximum_candidates")]
    pub maximum_generated_candidates: u32,
}

impl Default for PlanningSolverPolicy {
    fn default() -> Self {
        Self {
            backend: PlanningSolverBackend::MicroLp,
            deterministic_seed: 0,
            maximum_generated_candidates: default_maximum_candidates(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanArtifactOptions {
    #[serde(default = "default_true")]
    pub duckdb: bool,
    #[serde(default = "default_true")]
    pub rerun_rrd: bool,
}

impl Default for PlanArtifactOptions {
    fn default() -> Self {
        Self {
            duckdb: true,
            rerun_rrd: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanAuthority {
    pub principal_id: PrincipalId,
    pub work_context: WorkContextId,
    pub policy_revision: PolicyVersion,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Optimal,
    Partial,
    Infeasible,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GovernedPlan {
    pub schema_version: u64,
    pub plan_id: PlanId,
    pub resource_uri: String,
    pub status: PlanStatus,
    pub assignments: Vec<PlanAssignment>,
    pub requirements: Vec<PlanRequirementResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<PlanFinding>,
    pub source_map_releases: BTreeSet<MapReleaseUri>,
    pub frame_world_revision: FrameWorldRevisionUri,
    pub mobility_profiles: BTreeSet<MapMobilityProfileUri>,
    pub objective_values: PlanObjectiveValues,
    pub metrics: PlanMetrics,
    pub solver: PlanSolverSummary,
    pub algorithm_revision: String,
    pub request_digest_sha256: String,
    pub plan_digest_sha256: String,
    pub authority: PlanAuthority,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanAssignment {
    pub assignment_id: PlanAssignmentId,
    pub task_id: SpatialTaskId,
    pub ordinal: u32,
    pub agent_ids: BTreeSet<AgentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_id: Option<AgentGroupId>,
    pub mobility_profiles: BTreeSet<MapMobilityProfileUri>,
    pub target: MapGeometryReference,
    pub execution: PlanExecutionReference,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane_id: Option<LaneId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_band_id: Option<ResourceBandId>,
    pub timing: TaskTiming,
    pub recurrence: PlanRecurrence,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub shared_resources: BTreeMap<SharedResourceId, NonNegative>,
    pub cost: f64,
    pub risk: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RequirementSatisfaction {
    Complete,
    Partial,
    Unmet,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanRequirementResult {
    pub task_id: SpatialTaskId,
    pub minimum_quantity: u32,
    pub desired_quantity: u32,
    pub assigned_quantity: u32,
    pub satisfaction: RequirementSatisfaction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanFindingCode {
    NoEligibleUnit,
    InsufficientEligibleUnits,
    HardMinimumUnsatisfied,
    DesiredQuantityUnsatisfied,
    SolverInfeasible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanFinding {
    pub code: PlanFindingCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<SpatialTaskId>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanObjectiveValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weighted_objective: Option<f64>,
    pub priority_utility: f64,
    pub cost_penalty: f64,
    pub risk_penalty: f64,
    pub confidence_utility: f64,
    pub resource_penalty: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanMetrics {
    pub agents: u64,
    pub groups: u64,
    pub tasks: u64,
    pub generated_candidates: u64,
    pub assignments: u64,
    pub complete_requirements: u64,
    pub partial_requirements: u64,
    pub unmet_requirements: u64,
    pub total_cost: f64,
    pub total_risk: f64,
    pub total_confidence: f64,
    pub shared_resource_usage: BTreeMap<SharedResourceId, f64>,
    pub lane_usage: BTreeMap<LaneId, u32>,
    pub resource_band_usage: BTreeMap<ResourceBandId, u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanSolverSummary {
    pub backend: PlanningSolverBackend,
    pub algorithm_revision: String,
    pub deterministic_seed: u64,
    pub variables: u64,
    pub constraints: u64,
    pub generated_candidates: u64,
    pub termination: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanOutput {
    pub plan: GovernedPlan,
    pub plan_artifact: ArtifactMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duckdb_artifact: Option<ArtifactMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rrd_artifact: Option<ArtifactMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanContractError {
    InvalidIdentifier(&'static str),
    InvalidUri(&'static str),
    InvalidNumber(&'static str),
}

impl fmt::Display for PlanContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidIdentifier(label) => write!(formatter, "{label} is invalid"),
            Self::InvalidUri(label) => write!(formatter, "{label} is invalid"),
            Self::InvalidNumber(label) => write!(formatter, "{label} is invalid"),
        }
    }
}

impl std::error::Error for PlanContractError {}

fn default_max_assignments() -> u32 {
    1
}

fn default_maximum_candidates() -> u32 {
    10_000
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_references_reject_mutable_or_wrong_server_resources() {
        assert!(MapReleaseUri::parse("map://dataset/roads/release/release-1").is_ok());
        assert!(MapReleaseUri::parse("map://active-releases").is_err());
        assert!(MapSourceFeatureUri::parse("view://source-feature/a/b").is_err());
        assert!(MapMobilityProfileUri::parse("map://mobility-profile/aircraft/0").is_err());
    }

    #[test]
    fn stable_plan_ids_are_uuid_v5_values() {
        let first = PlanId::from_stable_key(b"task:request");
        let second = PlanId::from_stable_key(b"task:request");
        assert_eq!(first, second);
        assert!(PlanId::parse(first.to_string()).is_ok());
    }

    #[test]
    fn task_timing_detects_strict_window_overlap() {
        let time = |seconds| DateTime::from_timestamp(seconds, 0).unwrap();
        let first = TaskTiming::FixedWindow {
            start: time(10),
            end: time(20),
        };
        let touching = TaskTiming::FixedWindow {
            start: time(20),
            end: time(30),
        };
        let overlap = TaskTiming::FixedWindow {
            start: time(19),
            end: time(30),
        };
        assert!(!first.overlaps(&touching));
        assert!(first.overlaps(&overlap));
    }
}
