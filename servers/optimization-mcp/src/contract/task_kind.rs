//! Code-owned Task operations, independent of the database and MCP runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(task_type)]
pub enum OptimizationTaskKind {
    #[vocabulary(rename = "optimize_routes")]
    OptimizeRoutes,
    #[vocabulary(rename = "optimize_route_scenarios")]
    OptimizeRouteScenarios,
    #[vocabulary(rename = "solve_convex")]
    SolveConvex,
    #[vocabulary(rename = "solve_milp")]
    SolveMilp,
    #[vocabulary(rename = "verify_solution")]
    VerifySolution,
}
