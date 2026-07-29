use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{ArtifactMetadata, PolicyVersion, PrincipalId, WorkContextId};

use super::{
    CapacityDimensionId, ConstraintId, FiniteF64, LocationId, NonNegativeF64,
    OptimizationProblemUri, OptimizationProfileUri, OptimizationRunUri, OptimizationSolutionUri,
    OrderId, ProblemId, RouteCaseId, RouteObjectiveMetric, RunId, SolutionId, UnitInterval,
    VariableId, VehicleId, VerificationId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizationAuthority {
    pub principal_id: PrincipalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_context: Option<WorkContextId>,
    pub policy_revision: PolicyVersion,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProblemFamily {
    Routing,
    RouteScenarios,
    Convex,
    Milp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Preparing,
    Staging,
    Queued,
    Solving,
    Verifying,
    Publishing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizationProblemRecord {
    pub problem_id: ProblemId,
    pub problem_uri: OptimizationProblemUri,
    pub family: ProblemFamily,
    pub schema_version: String,
    pub digest_sha256: String,
    pub dimensions: ProblemDimensions,
    pub authority: OptimizationAuthority,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct ProblemDimensions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locations: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orders: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicles: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonzeros: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizationRunRecord {
    pub run_id: RunId,
    pub run_uri: OptimizationRunUri,
    pub problem_uri: OptimizationProblemUri,
    pub family: ProblemFamily,
    pub phase: RunPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incumbent: Option<IncumbentSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solution_uri: Option<OptimizationSolutionUri>,
    pub engine: EngineProvenance,
    pub timings: RunTimings,
    pub authority: OptimizationAuthority,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IncumbentSummary {
    pub sequence: u64,
    pub objective: FiniteF64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_bound: Option<FiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_gap: Option<UnitInterval>,
    pub found_at_seconds: NonNegativeF64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EngineProvenance {
    pub name: String,
    pub version: String,
    pub container_digest: String,
    pub executor_protocol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu_uuid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compute_capability: Option<String>,
    pub solver_profile_uri: OptimizationProfileUri,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, Default)]
pub struct RunTimings {
    #[serde(default)]
    pub queue_seconds: NonNegativeF64,
    #[serde(default)]
    pub preparation_seconds: NonNegativeF64,
    #[serde(default)]
    pub transfer_seconds: NonNegativeF64,
    #[serde(default)]
    pub solve_seconds: NonNegativeF64,
    #[serde(default)]
    pub verification_seconds: NonNegativeF64,
    #[serde(default)]
    pub publication_seconds: NonNegativeF64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SolutionFeasibility {
    Feasible,
    Partial,
    Infeasible,
    Unbounded,
    NoSolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SolverTermination {
    Completed,
    Optimal,
    TimeLimit,
    WorkLimit,
    NodeLimit,
    IterationLimit,
    Infeasible,
    Unbounded,
    NumericalFailure,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteStopResult {
    pub sequence: u32,
    pub order_id: Option<OrderId>,
    pub location_id: LocationId,
    pub node_kind: RouteNodeKind,
    pub arrival: NonNegativeF64,
    pub departure: NonNegativeF64,
    pub cumulative_cost: NonNegativeF64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub load: BTreeMap<CapacityDimensionId, i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RouteNodeKind {
    Depot,
    Service,
    Pickup,
    Delivery,
    Break,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VehicleRoute {
    pub vehicle_id: VehicleId,
    pub stops: Vec<RouteStopResult>,
    pub objective: FiniteF64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RouteSolutionSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<RouteCaseId>,
    pub vehicles_used: u32,
    pub orders_served: u32,
    pub orders_dropped: u32,
    pub undeliverable_orders: Vec<OrderId>,
    pub objective: FiniteF64,
    pub objective_components: BTreeMap<RouteObjectiveMetric, FiniteF64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MathematicalQuality {
    pub proven_optimal: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primal_objective: Option<FiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual_objective: Option<FiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_bound: Option<FiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absolute_gap: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_gap: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primal_residual: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual_residual: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_constraint_violation: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_integrality_violation: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_bound_violation: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iterations: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VariableValue {
    pub variable_id: VariableId,
    pub value: FiniteF64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintValue {
    pub constraint_id: ConstraintId,
    pub activity: FiniteF64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual_value: Option<FiniteF64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerificationFinding {
    pub code: VerificationCode,
    pub severity: VerificationSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable_id: Option<VariableId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraint_id: Option<ConstraintId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<OrderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicle_id: Option<VehicleId>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum VerificationCode {
    MissingVariable,
    DuplicateVariable,
    UnknownVariable,
    VariableLowerBound,
    VariableUpperBound,
    VariableIntegrality,
    ConstraintLowerBound,
    ConstraintUpperBound,
    ObjectiveMismatch,
    UnknownVehicle,
    DuplicateVehicleRoute,
    InvalidRouteEndpoint,
    UnknownRouteNode,
    DuplicateRouteNode,
    MissingMandatoryOrder,
    PartialPickupDelivery,
    PickupDeliveryPrecedence,
    VehicleOrderRestriction,
    OrderTimeWindow,
    VehicleTimeWindow,
    VehicleCapacity,
    VehicleMaximumCost,
    VehicleMaximumTime,
    UnavailableTravelArc,
    ArrivalSequence,
    SolverReportedFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSeverity {
    Information,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerificationReport {
    pub verification_id: VerificationId,
    pub verified: bool,
    pub findings: Vec<VerificationFinding>,
    pub absolute_tolerance: NonNegativeF64,
    pub relative_tolerance: NonNegativeF64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_constraint_violation: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_integrality_violation: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_bound_violation: Option<NonNegativeF64>,
    pub verified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum SolutionDetail {
    Routing {
        summaries: Vec<RouteSolutionSummary>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        routes: Vec<VehicleRoute>,
    },
    Convex {
        quality: MathematicalQuality,
        variables: Vec<VariableValue>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constraints: Vec<ConstraintValue>,
    },
    Milp {
        quality: MathematicalQuality,
        variables: Vec<VariableValue>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constraints: Vec<ConstraintValue>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        incumbents: Vec<IncumbentSummary>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizationSolution {
    pub solution_id: SolutionId,
    pub solution_uri: OptimizationSolutionUri,
    pub run_id: RunId,
    pub problem_uri: OptimizationProblemUri,
    pub feasibility: SolutionFeasibility,
    pub termination: SolverTermination,
    pub detail: SolutionDetail,
    pub verification: VerificationReport,
    pub engine: EngineProvenance,
    pub timings: RunTimings,
    pub digest_sha256: String,
    pub authority: OptimizationAuthority,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OptimizationToolOutput {
    pub run_uri: OptimizationRunUri,
    pub problem_uri: OptimizationProblemUri,
    pub solution_uri: OptimizationSolutionUri,
    pub family: ProblemFamily,
    pub feasibility: SolutionFeasibility,
    pub termination: SolverTermination,
    pub summary: OptimizationToolSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum OptimizationToolSummary {
    Routing { cases: Vec<RouteSolutionSummary> },
    Convex { quality: MathematicalQuality },
    Milp { quality: MathematicalQuality },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct VerifySolutionOutput {
    pub solution_uri: OptimizationSolutionUri,
    pub report: VerificationReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_artifact: Option<ArtifactMetadata>,
}
