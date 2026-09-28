//! Optimization-owned retained Task requests. Runtime-only; never part of the public wire contract.
use crate::contract::OptimizationTaskKind;
use crate::{
    contract::{
        OptimizationProfileUri, OptimizationSolution, OptimizeRouteScenariosRequest,
        OptimizeRoutesRequest, ProblemFamily, ProblemId, RunId, SolveConvexRequest,
        SolveMilpRequest, VerifySolutionRequest,
    },
    problem_store::PreparedProblemRef,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::IssuedArtifactWriteCapability;
use veoveo_types::TaskTypeDefinition;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SolveTaskCommon {
    pub problem_id: ProblemId,
    pub run_id: RunId,
    pub family: ProblemFamily,
    pub profile_uri: OptimizationProfileUri,
    pub submitted_at: DateTime<Utc>,
    pub prepared: PreparedProblemRef,
    pub artifact_write_capability: IssuedArtifactWriteCapability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PreparedVerifyTask {
    pub input: VerifySolutionRequest,
    pub solution: OptimizationSolution,
    pub prepared: PreparedProblemRef,
    pub submitted_at: DateTime<Utc>,
    pub artifact_write_capability: IssuedArtifactWriteCapability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OptimizationTaskRequest {
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
        request: Box<PreparedVerifyTask>,
    },
}

impl OptimizationTaskRequest {
    pub fn task_type(&self) -> veoveo_types::TaskTypeName {
        match self {
            Self::OptimizeRoutes { .. } => OptimizationTaskKind::OptimizeRoutes.name(),
            Self::OptimizeRouteScenarios { .. } => {
                OptimizationTaskKind::OptimizeRouteScenarios.name()
            }
            Self::SolveConvex { .. } => OptimizationTaskKind::SolveConvex.name(),
            Self::SolveMilp { .. } => OptimizationTaskKind::SolveMilp.name(),
            Self::VerifySolution { .. } => OptimizationTaskKind::VerifySolution.name(),
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
