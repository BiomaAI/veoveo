use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::contract::{
    CapacityDimensionId, ConstraintId, EXECUTOR_PROTOCOL_VERSION, FiniteF64, LocationId,
    NonNegativeF64, ObjectiveDirection, OrderId, ProblemFamily, RouteCaseId, RouteNodeKind,
    RouteObjectiveMetric, RunId, VariableId, VariableKind, VehicleId, VerificationFinding,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRequest {
    pub protocol: String,
    pub run_id: RunId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<ExecutorProfile>,
    pub operation: ExecutorOperation,
}

impl ExecutorRequest {
    pub fn new(run_id: RunId, profile: ExecutorProfile, operation: ExecutorOperation) -> Self {
        Self {
            protocol: EXECUTOR_PROTOCOL_VERSION.to_owned(),
            run_id,
            profile: Some(profile),
            operation,
        }
    }

    pub fn control(run_id: RunId, operation: ExecutorOperation) -> Self {
        Self {
            protocol: EXECUTOR_PROTOCOL_VERSION.to_owned(),
            run_id,
            profile: None,
            operation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutorOperation {
    Health,
    Cancel {
        target_run_id: RunId,
    },
    SolveRoutes {
        problem: CompiledRoutingProblem,
    },
    SolveRouteScenarios {
        cases: Vec<CompiledRouteCase>,
    },
    SolveModel {
        family: ExecutorModelFamily,
        model: CompiledMathematicalModel,
    },
    SolveModelFile {
        family: ExecutorModelFamily,
        staged_path: String,
    },
}
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum ExecutorOperationWire {
    Health {},
    Cancel {
        target_run_id: RunId,
    },
    SolveRoutes {
        problem: CompiledRoutingProblem,
    },
    SolveRouteScenarios {
        cases: Vec<CompiledRouteCase>,
    },
    SolveModel {
        family: ExecutorModelFamily,
        model: CompiledMathematicalModel,
    },
    SolveModelFile {
        family: ExecutorModelFamily,
        staged_path: String,
    },
}

impl<'de> Deserialize<'de> for ExecutorOperation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match ExecutorOperationWire::deserialize(deserializer)? {
            ExecutorOperationWire::Health {} => Self::Health,
            ExecutorOperationWire::Cancel { target_run_id } => Self::Cancel { target_run_id },
            ExecutorOperationWire::SolveRoutes { problem } => Self::SolveRoutes { problem },
            ExecutorOperationWire::SolveRouteScenarios { cases } => {
                Self::SolveRouteScenarios { cases }
            }
            ExecutorOperationWire::SolveModel { family, model } => {
                Self::SolveModel { family, model }
            }
            ExecutorOperationWire::SolveModelFile {
                family,
                staged_path,
            } => Self::SolveModelFile {
                family,
                staged_path,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorProfile {
    pub name: String,
    pub routing: RoutingSolverSettings,
    pub convex: ConvexSolverSettings,
    pub milp: MilpSolverSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutingSolverSettings {
    pub time_limit_seconds: NonNegativeF64,
    #[serde(default)]
    pub verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConvexMethod {
    Pdlp,
    Barrier,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConvexSolverSettings {
    pub time_limit_seconds: NonNegativeF64,
    pub method: ConvexMethod,
    pub optimality_tolerance: NonNegativeF64,
    pub presolve: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MilpSolverSettings {
    pub time_limit_seconds: NonNegativeF64,
    pub relative_gap: NonNegativeF64,
    pub absolute_gap: NonNegativeF64,
    pub integrality_tolerance: NonNegativeF64,
    pub presolve: bool,
    pub retain_incumbents: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledRouteCase {
    pub case_id: RouteCaseId,
    pub problem: CompiledRoutingProblem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledRoutingProblem {
    pub location_ids: Vec<LocationId>,
    pub nodes: Vec<CompiledRouteNode>,
    pub vehicles: Vec<CompiledVehicle>,
    pub vehicle_type_ids: Vec<String>,
    pub cost_matrices: Vec<CompiledDenseMatrix>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transit_time_matrices: Vec<CompiledDenseMatrix>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capacity_dimensions: Vec<CompiledCapacityDimension>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pickup_delivery_pairs: Vec<CompiledPickupDeliveryPair>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order_vehicle_matches: Vec<CompiledOrderVehicleMatch>,
    pub objectives: Vec<CompiledRouteObjective>,
    pub minimum_vehicles: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_solution: Option<CompiledInitialRoutingSolution>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledInitialRoutingSolution {
    pub vehicle_indices: Vec<u32>,
    pub route_nodes: Vec<u32>,
    pub node_kinds: Vec<CompiledInitialRouteNodeKind>,
    pub solution_offsets: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompiledInitialRouteNodeKind {
    Depot,
    Delivery,
    Pickup,
    Break,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledRouteNode {
    pub order_id: OrderId,
    pub location_id: LocationId,
    pub location_index: u32,
    pub kind: RouteNodeKind,
    pub service_duration: u32,
    pub earliest: u32,
    pub latest: u32,
    pub prize: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledVehicle {
    pub vehicle_id: VehicleId,
    pub vehicle_type: u8,
    pub start_location: u32,
    pub end_location: u32,
    pub earliest: u32,
    pub latest: u32,
    pub fixed_cost: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_cost: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_time: Option<f32>,
    pub omit_first_trip: bool,
    pub omit_last_trip: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breaks: Vec<CompiledVehicleBreak>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledVehicleBreak {
    pub earliest: u32,
    pub latest: u32,
    pub duration: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_locations: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledDenseMatrix {
    pub vehicle_type: u8,
    pub dimension: u32,
    pub values: Vec<f32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unavailable_cells: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledCapacityDimension {
    pub dimension_id: CapacityDimensionId,
    pub demand: Vec<i32>,
    pub capacity: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledPickupDeliveryPair {
    pub pickup_node: u32,
    pub delivery_node: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledOrderVehicleMatch {
    pub node: u32,
    pub vehicles: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledRouteObjective {
    pub metric: RouteObjectiveMetric,
    pub weight: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorModelFamily {
    Convex,
    Milp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledMathematicalModel {
    pub variable_ids: Vec<VariableId>,
    pub variable_kinds: Vec<VariableKind>,
    pub variable_lower_bounds: Vec<Option<FiniteF64>>,
    pub variable_upper_bounds: Vec<Option<FiniteF64>>,
    pub objective_direction: ObjectiveDirection,
    pub objective_offset: FiniteF64,
    pub objective_coefficients: Vec<FiniteF64>,
    pub constraint_ids: Vec<ConstraintId>,
    pub constraint_matrix: CsrMatrix,
    pub constraint_lower_bounds: Vec<Option<FiniteF64>>,
    pub constraint_upper_bounds: Vec<Option<FiniteF64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quadratic_objective: Option<CsrMatrix>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quadratic_constraints: Vec<CompiledQuadraticConstraint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_primal_solution: Option<Vec<FiniteF64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_dual_solution: Option<Vec<FiniteF64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CsrMatrix {
    pub rows: u32,
    pub columns: u32,
    pub offsets: Vec<u32>,
    pub indices: Vec<u32>,
    pub values: Vec<FiniteF64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompiledQuadraticConstraint {
    pub constraint_id: ConstraintId,
    pub linear_indices: Vec<u32>,
    pub linear_values: Vec<FiniteF64>,
    pub rows: Vec<u32>,
    pub columns: Vec<u32>,
    pub values: Vec<FiniteF64>,
    pub sense: QuadraticConstraintSense,
    pub rhs: FiniteF64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuadraticConstraintSense {
    LessThanOrEqual,
    GreaterThanOrEqual,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorResponse {
    pub protocol: String,
    pub run_id: RunId,
    pub result: ExecutorResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutorResult {
    Health {
        health: ExecutorHealth,
    },
    Routes {
        solution: ExecutorRoutingSolution,
    },
    RouteScenarios {
        solutions: Vec<ExecutorRouteCaseSolution>,
    },
    Model {
        solution: ExecutorMathematicalSolution,
    },
    Error {
        error: ExecutorError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorHealth {
    pub ready: bool,
    pub cuopt_version: String,
    pub cuda_runtime_version: String,
    pub gpu_name: String,
    pub gpu_uuid: String,
    pub compute_capability: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorError {
    pub code: ExecutorErrorCode,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<VerificationFinding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorErrorCode {
    InvalidRequest,
    UnsupportedProblem,
    OutOfMemory,
    SolverFailure,
    ProtocolFailure,
    GpuUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorRoutingStatus {
    Success,
    Timeout,
    Infeasible,
    Failed,
    Empty,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRoutingSolution {
    pub status: ExecutorRoutingStatus,
    pub message: String,
    pub objective: FiniteF64,
    pub objective_components: BTreeMap<RouteObjectiveMetric, FiniteF64>,
    pub vehicles_used: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<ExecutorVehicleRoute>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undeliverable_nodes: Vec<u32>,
    pub solve_seconds: NonNegativeF64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRouteCaseSolution {
    pub case_id: RouteCaseId,
    pub solution: ExecutorRoutingSolution,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorVehicleRoute {
    pub vehicle: u32,
    pub nodes: Vec<ExecutorRouteVisit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRouteVisit {
    pub node: ExecutorRouteNode,
    pub arrival: NonNegativeF64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutorRouteNode {
    Depot { location: u32 },
    Order { node: u32 },
    Break { location: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorModelStatus {
    Optimal,
    Feasible,
    Infeasible,
    Unbounded,
    InfeasibleOrUnbounded,
    TimeLimit,
    IterationLimit,
    NodeLimit,
    NumericalFailure,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorMathematicalSolution {
    pub family: ProblemFamily,
    pub status: ExecutorModelStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primal_solution: Vec<FiniteF64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dual_solution: Vec<FiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primal_objective: Option<FiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual_objective: Option<FiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub best_bound: Option<FiniteF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_gap: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primal_residual: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dual_residual: Option<NonNegativeF64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iterations: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodes: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub incumbents: Vec<ExecutorIncumbent>,
    pub solve_seconds: NonNegativeF64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecutorIncumbent {
    pub sequence: u64,
    pub values: Vec<FiniteF64>,
    pub objective: FiniteF64,
    pub bound: FiniteF64,
    pub found_at_seconds: NonNegativeF64,
}

#[cfg(test)]
mod strict_protocol_tests {
    use super::*;

    #[test]
    fn shared_fixture_covers_every_operation_result_and_route_node_variant() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/executor-protocol.json")).unwrap();
        for request in fixture["requests"].as_array().unwrap() {
            let decoded: ExecutorRequest = serde_json::from_value(request.clone()).unwrap();
            let encoded = serde_json::to_value(&decoded).unwrap();
            assert_eq!(
                serde_json::from_value::<ExecutorRequest>(encoded).unwrap(),
                decoded
            );
        }
        for response in fixture["responses"].as_array().unwrap() {
            let decoded: ExecutorResponse = serde_json::from_value(response.clone()).unwrap();
            let encoded = serde_json::to_value(&decoded).unwrap();
            assert_eq!(
                serde_json::from_value::<ExecutorResponse>(encoded).unwrap(),
                decoded
            );
        }
    }

    #[test]
    fn shared_peer_fixture_rejects_unknown_request_and_response_fields() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../testdata/executor-protocol.json")).unwrap();
        let request = fixture["request"].clone();
        let response = fixture["response"].clone();
        assert!(serde_json::from_value::<ExecutorRequest>(request.clone()).is_ok());
        assert!(serde_json::from_value::<ExecutorResponse>(response.clone()).is_ok());
        for pointer in ["", "/operation"] {
            let mut changed = request.clone();
            changed
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), true.into());
            assert!(serde_json::from_value::<ExecutorRequest>(changed).is_err());
        }
        for pointer in ["", "/result", "/result/error"] {
            let mut changed = response.clone();
            changed
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), true.into());
            assert!(serde_json::from_value::<ExecutorResponse>(changed).is_err());
        }
    }
    #[test]
    fn private_protocol_schema_snapshot() {
        let snapshot = serde_json::json!({
            "request": schemars::generate::SchemaSettings::draft2020_12()
                .for_serialize().into_generator().into_root_schema_for::<ExecutorRequest>(),
            "response": schemars::generate::SchemaSettings::draft2020_12()
                .for_deserialize().into_generator().into_root_schema_for::<ExecutorResponse>(),
        });
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/private-protocol.schema.json");
        if std::env::var_os("UPDATE_PRIVATE_PROTOCOL_SCHEMAS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                &path,
                serde_json::to_string_pretty(&snapshot).unwrap() + "\n",
            )
            .unwrap();
        }
        let maintained: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&path)
                .expect("generate the maintained private protocol schema snapshot"),
        )
        .unwrap();
        assert_eq!(
            snapshot, maintained,
            "private protocol schema snapshot drift"
        );
    }
}
