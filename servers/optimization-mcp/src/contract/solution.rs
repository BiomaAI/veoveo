use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactMetadata;
use veoveo_types::{PolicyVersion, PrincipalId, WorkContextId};

use super::{
    CapacityDimensionId, ConstraintId, FiniteF64, LocationId, NonNegativeF64,
    OptimizationProblemUri, OptimizationProfileUri, OptimizationRunUri, OptimizationSolutionUri,
    OrderId, ProblemId, RouteCaseId, RouteObjectiveMetric, RunId, SolutionId, UnitInterval,
    VariableId, VehicleId, VerificationId,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[schemars(rename = "OptimizationProblemRecord")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizationProblemRecordValue {
    pub problem_id: ProblemId,
    pub problem_uri: OptimizationProblemUri,
    pub family: ProblemFamily,
    #[schemars(schema_with = "super::naming::problem_version")]
    pub schema_version: String,
    pub digest_sha256: veoveo_artifact_contract::UploadSha256,
    pub dimensions: ProblemDimensions,
    pub authority: OptimizationAuthority,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EngineProvenance {
    pub name: String,
    pub version: String,
    pub container_digest: String,
    #[schemars(schema_with = "super::naming::executor_version")]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VehicleRoute {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case_id: Option<RouteCaseId>,
    pub vehicle_id: VehicleId,
    pub stops: Vec<RouteStopResult>,
    pub objective: FiniteF64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VariableValue {
    pub variable_id: VariableId,
    pub value: FiniteF64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConstraintValue {
    pub constraint_id: ConstraintId,
    pub activity: FiniteF64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual_value: Option<FiniteF64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
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
#[schemars(rename = "VerificationReport")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationReportValue {
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
#[serde(
    tag = "family",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
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
#[schemars(rename = "OptimizationSolution")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizationSolutionValue {
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
    pub digest_sha256: veoveo_artifact_contract::UploadSha256,
    pub authority: OptimizationAuthority,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "OptimizationToolOutput")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizationToolOutputValue {
    pub run_uri: OptimizationRunUri,
    pub problem_uri: OptimizationProblemUri,
    pub result_uri: OptimizationSolutionUri,
    pub family: ProblemFamily,
    pub feasibility: SolutionFeasibility,
    pub termination: SolverTermination,
    pub summary: OptimizationToolSummary,
    pub problem_artifact: ArtifactMetadata,
    pub solution_artifact: ArtifactMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "family",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OptimizationProblemDefinition {
    Routing { problem: super::RoutingProblem },
    RouteScenarios { cases: Vec<super::RouteScenario> },
    Convex { problem: super::ConvexProblem },
    Milp { problem: super::MilpProblem },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "OptimizationProblemResource")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OptimizationProblemResourceValue {
    pub record: OptimizationProblemRecord,
    pub definition: OptimizationProblemDefinition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "family",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OptimizationToolSummary {
    Routing { cases: Vec<RouteSolutionSummary> },
    Convex { quality: MathematicalQuality },
    Milp { quality: MathematicalQuality },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "VerifySolutionOutput")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifySolutionOutputValue {
    pub solution_uri: OptimizationSolutionUri,
    pub report: VerificationReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_artifact: Option<ArtifactMetadata>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "OptimizationProblemRecordValue",
    into = "OptimizationProblemRecordValue"
)]
pub struct OptimizationProblemRecord(veoveo_types::Checked<OptimizationProblemRecordValue>);
impl std::ops::Deref for OptimizationProblemRecord {
    type Target = OptimizationProblemRecordValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for OptimizationProblemRecord {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "OptimizationProblemRecord".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        OptimizationProblemRecordValue::json_schema(generator)
    }
}
impl TryFrom<OptimizationProblemRecordValue> for OptimizationProblemRecord {
    type Error = super::OptimizationContractError;
    fn try_from(value: OptimizationProblemRecordValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<OptimizationProblemRecord> for OptimizationProblemRecordValue {
    fn from(value: OptimizationProblemRecord) -> Self {
        value.0.into_inner()
    }
}
impl OptimizationProblemRecordValue {
    pub fn build(self) -> Result<OptimizationProblemRecord, super::OptimizationContractError> {
        self.try_into()
    }
}
impl veoveo_types::Check for OptimizationProblemRecordValue {
    type Error = super::OptimizationContractError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.problem_uri.id() != &self.problem_id
            || self.schema_version
                != match self.family {
                    ProblemFamily::Routing | ProblemFamily::RouteScenarios => {
                        super::ROUTING_PROBLEM_VERSION
                    }
                    ProblemFamily::Convex => super::CONVEX_PROBLEM_VERSION,
                    ProblemFamily::Milp => super::MILP_PROBLEM_VERSION,
                }
        {
            return Err(super::value_admission::mismatch());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VerificationReportValue", into = "VerificationReportValue")]
pub struct VerificationReport(veoveo_types::Checked<VerificationReportValue>);
impl std::ops::Deref for VerificationReport {
    type Target = VerificationReportValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for VerificationReport {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "VerificationReport".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        VerificationReportValue::json_schema(generator)
    }
}
impl TryFrom<VerificationReportValue> for VerificationReport {
    type Error = super::OptimizationContractError;
    fn try_from(value: VerificationReportValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<VerificationReport> for VerificationReportValue {
    fn from(value: VerificationReport) -> Self {
        value.0.into_inner()
    }
}
impl VerificationReportValue {
    pub fn build(self) -> Result<VerificationReport, super::OptimizationContractError> {
        self.try_into()
    }
}
impl veoveo_types::Check for VerificationReportValue {
    type Error = super::OptimizationContractError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.verified
            == self
                .findings
                .iter()
                .any(|finding| finding.severity == VerificationSeverity::Error)
        {
            return Err(super::value_admission::mismatch());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "OptimizationSolutionValue",
    into = "OptimizationSolutionValue"
)]
pub struct OptimizationSolution(veoveo_types::Checked<OptimizationSolutionValue>);
impl std::ops::Deref for OptimizationSolution {
    type Target = OptimizationSolutionValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for OptimizationSolution {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "OptimizationSolution".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        OptimizationSolutionValue::json_schema(generator)
    }
}
impl TryFrom<OptimizationSolutionValue> for OptimizationSolution {
    type Error = super::OptimizationContractError;
    fn try_from(value: OptimizationSolutionValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<OptimizationSolution> for OptimizationSolutionValue {
    fn from(value: OptimizationSolution) -> Self {
        value.0.into_inner()
    }
}
impl OptimizationSolutionValue {
    pub fn build(self) -> Result<OptimizationSolution, super::OptimizationContractError> {
        self.try_into()
    }
}
impl veoveo_types::Check for OptimizationSolutionValue {
    type Error = super::OptimizationContractError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.engine.executor_protocol != super::EXECUTOR_PROTOCOL_VERSION
            || self.solution_uri.id() != &self.solution_id
            || self.digest_sha256 != super::value_admission::solution_digest(self)?
        {
            return Err(super::value_admission::mismatch());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "OptimizationProblemResourceValue",
    into = "OptimizationProblemResourceValue"
)]
pub struct OptimizationProblemResource(veoveo_types::Checked<OptimizationProblemResourceValue>);
impl std::ops::Deref for OptimizationProblemResource {
    type Target = OptimizationProblemResourceValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for OptimizationProblemResource {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "OptimizationProblemResource".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        OptimizationProblemResourceValue::json_schema(generator)
    }
}
impl TryFrom<OptimizationProblemResourceValue> for OptimizationProblemResource {
    type Error = super::OptimizationContractError;
    fn try_from(value: OptimizationProblemResourceValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<OptimizationProblemResource> for OptimizationProblemResourceValue {
    fn from(value: OptimizationProblemResource) -> Self {
        value.0.into_inner()
    }
}
impl OptimizationProblemResourceValue {
    pub fn build(self) -> Result<OptimizationProblemResource, super::OptimizationContractError> {
        self.try_into()
    }
}
impl veoveo_types::Check for OptimizationProblemResourceValue {
    type Error = super::OptimizationContractError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.record.family != self.definition.family()
            || self.record.dimensions != self.definition.dimensions()?
            || self.record.digest_sha256
                != super::value_admission::definition_digest(&self.definition)?
        {
            return Err(super::value_admission::mismatch());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "OptimizationToolOutputValue",
    into = "OptimizationToolOutputValue"
)]
pub struct OptimizationToolOutput(veoveo_types::Checked<OptimizationToolOutputValue>);
impl std::ops::Deref for OptimizationToolOutput {
    type Target = OptimizationToolOutputValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for OptimizationToolOutput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "OptimizationToolOutput".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        OptimizationToolOutputValue::json_schema(generator)
    }
}
impl TryFrom<OptimizationToolOutputValue> for OptimizationToolOutput {
    type Error = super::OptimizationContractError;
    fn try_from(value: OptimizationToolOutputValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<OptimizationToolOutput> for OptimizationToolOutputValue {
    fn from(value: OptimizationToolOutput) -> Self {
        value.0.into_inner()
    }
}
impl OptimizationToolOutputValue {
    pub fn build(self) -> Result<OptimizationToolOutput, super::OptimizationContractError> {
        self.try_into()
    }
}
impl veoveo_types::Check for OptimizationToolOutputValue {
    type Error = super::OptimizationContractError;
    fn check(&self) -> Result<(), Self::Error> {
        if !matches!(
            (self.family, &self.summary),
            (
                ProblemFamily::Routing | ProblemFamily::RouteScenarios,
                OptimizationToolSummary::Routing { .. }
            ) | (
                ProblemFamily::Convex,
                OptimizationToolSummary::Convex { .. }
            ) | (ProblemFamily::Milp, OptimizationToolSummary::Milp { .. })
        ) {
            return Err(super::value_admission::mismatch());
        }
        super::value_admission::artifact(&self.problem_artifact)?;
        super::value_admission::artifact(&self.solution_artifact)?;
        let mut ids = std::collections::BTreeSet::new();
        for artifact in std::iter::once(&self.problem_artifact)
            .chain(std::iter::once(&self.solution_artifact))
            .chain(&self.artifacts)
        {
            super::value_admission::artifact(artifact)?;
            if !ids.insert(artifact.artifact_id()) {
                return Err(super::value_admission::mismatch());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "VerifySolutionOutputValue",
    into = "VerifySolutionOutputValue"
)]
pub struct VerifySolutionOutput(veoveo_types::Checked<VerifySolutionOutputValue>);
impl std::ops::Deref for VerifySolutionOutput {
    type Target = VerifySolutionOutputValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for VerifySolutionOutput {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "VerifySolutionOutput".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        VerifySolutionOutputValue::json_schema(generator)
    }
}
impl TryFrom<VerifySolutionOutputValue> for VerifySolutionOutput {
    type Error = super::OptimizationContractError;
    fn try_from(value: VerifySolutionOutputValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<VerifySolutionOutput> for VerifySolutionOutputValue {
    fn from(value: VerifySolutionOutput) -> Self {
        value.0.into_inner()
    }
}
impl VerifySolutionOutputValue {
    pub fn build(self) -> Result<VerifySolutionOutput, super::OptimizationContractError> {
        self.try_into()
    }
}
impl veoveo_types::Check for VerifySolutionOutputValue {
    type Error = super::OptimizationContractError;
    fn check(&self) -> Result<(), Self::Error> {
        if let Some(artifact) = &self.report_artifact {
            super::value_admission::artifact(artifact)?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunWire {
    run_id: RunId,
    run_uri: OptimizationRunUri,
    problem_uri: OptimizationProblemUri,
    family: ProblemFamily,
    phase: RunPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    incumbent: Option<IncumbentSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    solution_uri: Option<OptimizationSolutionUri>,
    engine: EngineProvenance,
    timings: RunTimings,
    authority: OptimizationAuthority,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl<'de> Deserialize<'de> for OptimizationRunRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = RunWire::deserialize(deserializer)?;
        Self {
            run_id: value.run_id,
            run_uri: value.run_uri,
            problem_uri: value.problem_uri,
            family: value.family,
            phase: value.phase,
            incumbent: value.incumbent,
            solution_uri: value.solution_uri,
            engine: value.engine,
            timings: value.timings,
            authority: value.authority,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
        .build()
        .map_err(serde::de::Error::custom)
    }
}
impl OptimizationRunRecord {
    pub fn build(self) -> Result<Self, super::OptimizationContractError> {
        veoveo_types::Check::check(&self)?;
        Ok(self)
    }
}
impl veoveo_types::Check for OptimizationRunRecord {
    type Error = super::OptimizationContractError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.engine.executor_protocol != super::EXECUTOR_PROTOCOL_VERSION
            || self.run_uri.id() != &self.run_id
            || self.updated_at < self.created_at
        {
            return Err(super::value_admission::mismatch());
        }
        Ok(())
    }
}

#[cfg(test)]
mod terminal_contract_tests {
    use super::OptimizationToolOutput;

    #[test]
    fn solve_output_schema_has_one_canonical_result_handoff() {
        let schema = serde_json::to_value(schemars::schema_for!(OptimizationToolOutput)).unwrap();
        let properties = schema["properties"].as_object().unwrap();

        assert!(properties.contains_key("resultUri"));
        assert!(!properties.contains_key("solutionUri"));
    }
}
