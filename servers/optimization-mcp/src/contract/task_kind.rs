//! Code-owned Task operations, independent of the database and MCP runtime.
veoveo_types::declare_task_types! {
    pub enum OptimizationTaskKind {
        OptimizeRoutes => "optimize_routes",
        OptimizeRouteScenarios => "optimize_route_scenarios",
        SolveConvex => "solve_convex",
        SolveMilp => "solve_milp",
        VerifySolution => "verify_solution",
    }
}
