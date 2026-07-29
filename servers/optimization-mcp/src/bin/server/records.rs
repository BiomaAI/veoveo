use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::IssuedArtifactWriteCapability;
use veoveo_optimization_mcp::{
    domain::{
        OptimizationProfileUri, OptimizationSolution, OptimizeRouteScenariosRequest,
        OptimizeRoutesRequest, ProblemFamily, ProblemId, RunId, SolveConvexRequest,
        SolveMilpRequest, VerifySolutionRequest,
    },
    problem_store::PreparedProblemRef,
};

pub(super) const OPTIMIZE_ROUTES_TASK: &str = "optimize_routes";
pub(super) const OPTIMIZE_ROUTE_SCENARIOS_TASK: &str = "optimize_route_scenarios";
pub(super) const SOLVE_CONVEX_TASK: &str = "solve_convex";
pub(super) const SOLVE_MILP_TASK: &str = "solve_milp";
pub(super) const VERIFY_SOLUTION_TASK: &str = "verify_solution";

pub(crate) const TASK_TOOLS: &[&str] = &[
    OPTIMIZE_ROUTES_TASK,
    OPTIMIZE_ROUTE_SCENARIOS_TASK,
    SOLVE_CONVEX_TASK,
    SOLVE_MILP_TASK,
    VERIFY_SOLUTION_TASK,
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct SolveTaskCommon {
    pub problem_id: ProblemId,
    pub run_id: RunId,
    pub family: ProblemFamily,
    pub profile_uri: OptimizationProfileUri,
    pub submitted_at: DateTime<Utc>,
    pub prepared: PreparedProblemRef,
    pub artifact_write_capability: IssuedArtifactWriteCapability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct PreparedVerifyTask {
    pub input: VerifySolutionRequest,
    pub solution: OptimizationSolution,
    pub prepared: PreparedProblemRef,
    pub submitted_at: DateTime<Utc>,
    pub artifact_write_capability: IssuedArtifactWriteCapability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum OptimizationTaskRequest {
    OptimizeRoutes {
        common: SolveTaskCommon,
        input: OptimizeRoutesRequest,
    },
    OptimizeRouteScenarios {
        common: SolveTaskCommon,
        input: OptimizeRouteScenariosRequest,
    },
    SolveConvex {
        common: SolveTaskCommon,
        input: SolveConvexRequest,
    },
    SolveMilp {
        common: SolveTaskCommon,
        input: SolveMilpRequest,
    },
    VerifySolution {
        request: PreparedVerifyTask,
    },
}

impl OptimizationTaskRequest {
    pub fn task_type(&self) -> &'static str {
        match self {
            Self::OptimizeRoutes { .. } => OPTIMIZE_ROUTES_TASK,
            Self::OptimizeRouteScenarios { .. } => OPTIMIZE_ROUTE_SCENARIOS_TASK,
            Self::SolveConvex { .. } => SOLVE_CONVEX_TASK,
            Self::SolveMilp { .. } => SOLVE_MILP_TASK,
            Self::VerifySolution { .. } => VERIFY_SOLUTION_TASK,
        }
    }

    pub fn common(&self) -> Option<&SolveTaskCommon> {
        match self {
            Self::OptimizeRoutes { common, .. }
            | Self::OptimizeRouteScenarios { common, .. }
            | Self::SolveConvex { common, .. }
            | Self::SolveMilp { common, .. } => Some(common),
            Self::VerifySolution { .. } => None,
        }
    }
}
